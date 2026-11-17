use crate::config::AppConfig;
use crate::generators::{account, card, customer, merchants, transaction};
use crate::models::customer::Customer;
use crate::reference::{self, ReferenceError};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

pub use crate::models::dataset::Dataset;

/// Everything needed to run the generator. All inputs are explicit so the run is
/// reproducible: the same `(config, GenParams)` yields byte-identical output.
#[derive(Debug, Clone)]
pub struct GenParams {
    pub seed: u64,
    pub customers: usize,
    pub days: i64,
    pub base_end_date: DateTime<Utc>,
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
}

impl GenParams {
    pub fn new(seed: u64, customers: usize, days: i64) -> Self {
        Self {
            seed,
            customers,
            days,
            base_end_date: Utc::now(),
            input_dir: PathBuf::from("references"),
            output_dir: PathBuf::from("out"),
        }
    }

    pub fn base_end_date(mut self, d: DateTime<Utc>) -> Self {
        self.base_end_date = d;
        self
    }

    pub fn input_dir(mut self, p: impl Into<PathBuf>) -> Self {
        self.input_dir = p.into();
        self
    }

    pub fn output_dir(mut self, p: impl Into<PathBuf>) -> Self {
        self.output_dir = p.into();
        self
    }
}

/// Generate the full dataset. Pure with respect to `(config, params)`.
///
/// Reference tables are resolved from `p.input_dir` and validated against the
/// [`crate::reference`] schema; a missing or malformed reference directory
/// returns a descriptive error instead of panicking.
pub fn generate_all(config: &AppConfig, p: &GenParams) -> Result<Dataset, ReferenceError> {
    let residential = p.input_dir.join(reference::RESIDENTIAL_FILE);
    let merchants_path = p.input_dir.join(reference::MERCHANTS_FILE);

    let customers =
        customer::generate_customers(p.customers, p.seed, p.base_end_date, config, &residential)?;
    let customer_ids: Vec<String> = customers.iter().map(|c| c.customer_id.clone()).collect();
    let accounts = account::generate_accounts(customer_ids, p.seed, p.base_end_date, config);
    let cards = card::generate_cards(&accounts, p.seed, p.base_end_date, config);

    let (merchant_refs, spatial) =
        merchants::build_merchant_index(reference::load_merchants(&merchants_path)?);
    let customer_map: HashMap<String, &Customer> = customers
        .iter()
        .map(|c| (c.customer_id.clone(), c))
        .collect();

    let (transactions, fraud_metadata) = transaction::generate_transactions_chunk(
        &cards,
        &customer_map,
        &spatial,
        &merchant_refs,
        config,
        p.base_end_date,
        p.days,
    );

    Ok(Dataset {
        customers,
        accounts,
        cards,
        transactions,
        fraud_metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn transactions_stay_within_the_requested_window() {
        let tmp = tempfile::tempdir().unwrap();
        crate::fixtures::write_reference_fixtures(tmp.path()).unwrap();

        let config = AppConfig::default_embedded();
        let end = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let days = 30i64;
        let params = GenParams {
            seed: 1,
            customers: 10,
            days,
            base_end_date: end,
            input_dir: tmp.path().to_path_buf(),
            output_dir: tmp.path().to_path_buf(),
        };

        let data = generate_all(&config, &params).unwrap();
        let start = end - Duration::days(days);
        // Fraud bursts warp timestamps by seconds, so allow a small overshoot.
        let upper = end + Duration::hours(1);

        for tx in &data.transactions {
            let ts = DateTime::parse_from_rfc3339(&tx.timestamp)
                .unwrap()
                .with_timezone(&Utc);
            assert!(
                ts >= start && ts <= upper,
                "transaction {ts} outside [{start}, {upper}] for a {days}-day window"
            );
        }
    }
}
