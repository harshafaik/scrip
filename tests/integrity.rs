use chrono::{DateTime, Utc};
use scrip::config::AppConfig;
use scrip::generators::run::{generate_all, GenParams};
use std::collections::HashSet;

#[test]
fn referential_integrity_and_invariants() {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    scrip::fixtures::write_reference_fixtures(&refs).unwrap();

    let cfg = AppConfig::default_embedded();
    let params = GenParams {
        seed: 7,
        customers: 20,
        days: 20,
        base_end_date: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        input_dir: refs,
        output_dir: tmp.path().join("out"),
    };
    let data = generate_all(&cfg, &params).unwrap();

    let customers: HashSet<_> = data
        .customers
        .iter()
        .map(|c| c.customer_id.clone())
        .collect();
    let accounts: HashSet<_> = data.accounts.iter().map(|a| a.account_id.clone()).collect();
    let cards: HashSet<_> = data.cards.iter().map(|c| c.card_id.clone()).collect();
    let txns: HashSet<_> = data
        .transactions
        .iter()
        .map(|t| t.transaction_id.clone())
        .collect();

    assert_eq!(
        customers.len(),
        data.customers.len(),
        "duplicate customer ids"
    );
    assert_eq!(accounts.len(), data.accounts.len(), "duplicate account ids");
    assert_eq!(cards.len(), data.cards.len(), "duplicate card ids");

    // Foreign keys resolve.
    assert!(data
        .accounts
        .iter()
        .all(|a| customers.contains(&a.customer_id)));
    assert!(data
        .cards
        .iter()
        .all(|c| accounts.contains(&c.account_id) && customers.contains(&c.customer_id)));
    assert!(data.transactions.iter().all(|t| cards.contains(&t.card_id)
        && accounts.contains(&t.account_id)
        && customers.contains(&t.customer_id)));
    assert!(data
        .fraud_metadata
        .iter()
        .all(|m| txns.contains(&m.transaction_id)));

    // Value invariants.
    assert!(data.transactions.iter().all(|t| t.amount > 0.0));
    assert!(data
        .fraud_metadata
        .iter()
        .all(|m| matches!(m.label_noise.as_str(), "none" | "fn" | "fp")));
    assert!(data
        .fraud_metadata
        .iter()
        .all(|m| !(m.label_noise == "fp" && m.fraud_target)));
}
