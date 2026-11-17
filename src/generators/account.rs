use crate::config::AppConfig;
use crate::models::account::Account;
use chrono::{DateTime, Utc};
use rayon::prelude::*;

/// Each customer is assigned a number of accounts drawn from
/// `control.accounts_per_customer` (inclusive range).
pub fn generate_accounts(
    customer_ids: Vec<String>,
    base_seed: u64,
    now: DateTime<Utc>,
    config: &AppConfig,
) -> Vec<Account> {
    let salt_account: u64 = 0x0000_AC02_0000_0002;
    let per_customer = &config.customer.control.accounts_per_customer;

    customer_ids
        .into_par_iter()
        .enumerate()
        .flat_map(|(i, cid)| {
            let mut rng = crate::core::rng::stream(base_seed, salt_account, i as u64);
            let count = per_customer.sample(&mut rng);

            (0..count)
                .map(|_| Account::new(cid.clone(), &mut rng, now))
                .collect::<Vec<_>>()
        })
        .collect()
}
