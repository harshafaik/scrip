use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub account_id: String,
    pub customer_id: String,

    pub bank_id: String,
    pub account_no: String,
    pub account_type: String,
    pub balance: f64,
    pub account_status: String,
    pub creation_date: String,
}

impl Account {
    #[must_use]
    pub fn new(customer_id: String, rng: &mut impl Rng, now: DateTime<Utc>) -> Self {
        let types = ["Savings", "Current", "Credit"];
        let selected_type = types[rng.random_range(0..types.len())].to_string();

        let end_date = now.date_naive();
        let start_date = end_date - chrono::Duration::days(365 * 3);
        let random_days = rng.random_range(0..(end_date - start_date).num_days());
        let open_date = start_date + chrono::Duration::days(random_days);

        Account {
            account_id: Uuid::from_u128(rng.random::<u128>()).to_string(),
            customer_id,
            bank_id: format!("Bank-{}", rng.random_range(1000..9999)),
            account_no: rng
                .random_range(1000_0000_0000_u64..9999_9999_9999_u64)
                .to_string(),
            account_type: selected_type,
            balance: rng.random_range(1000.00..500_000.00),
            account_status: "Active".to_string(),
            creation_date: open_date.to_string(),
        }
    }
}
