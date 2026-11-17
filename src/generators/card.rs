use crate::config::AppConfig;
use crate::models::account::Account;
use crate::models::card::Card;
use chrono::{DateTime, Utc};
use rayon::prelude::*;

/// Each account is issued a number of cards drawn from
/// `control.cards_per_account` (inclusive range).
pub fn generate_cards(
    accounts: &[Account],
    base_seed: u64,
    now: DateTime<Utc>,
    config: &AppConfig,
) -> Vec<Card> {
    let salt_card: u64 = 0x00CA_0000_0000_0003;
    let per_account = &config.customer.control.cards_per_account;

    accounts
        .par_iter()
        .enumerate()
        .flat_map(|(i, acc)| {
            let mut rng = crate::core::rng::stream(base_seed, salt_card, i as u64);
            let count = per_account.sample(&mut rng);

            (0..count)
                .map(|_| {
                    Card::new(
                        acc.account_id.clone(),
                        acc.customer_id.clone(),
                        acc.bank_id.clone(),
                        &mut rng,
                        now,
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
