use chrono::{DateTime, Utc};
use scrip::config::AppConfig;
use scrip::fixtures;
use scrip::generators::run::{generate_all, GenParams};
use std::net::IpAddr;

#[test]
fn every_transaction_ip_is_a_valid_address() {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    fixtures::write_reference_fixtures(&refs).unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-31T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let params = GenParams {
        seed: 7,
        customers: 100,
        days: 30,
        base_end_date: base,
        input_dir: refs,
        output_dir: tmp.path().join("out"),
    };

    let data = generate_all(&AppConfig::default_embedded(), &params).unwrap();
    assert!(!data.transactions.is_empty());

    for tx in &data.transactions {
        assert!(
            !tx.ip_address.contains('/'),
            "subnet leaked into ip_address: {}",
            tx.ip_address
        );
        tx.ip_address
            .parse::<IpAddr>()
            .unwrap_or_else(|_| panic!("invalid ip_address: {}", tx.ip_address));
    }
}
