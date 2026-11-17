use chrono::{DateTime, Utc};
use scrip::config::{AppConfig, RangeControl};
use scrip::generators::run::{generate_all, Dataset, GenParams};
use std::path::Path;

fn fixed_date() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn run_with(tmp: &Path, accounts: RangeControl, cards: RangeControl) -> Dataset {
    let mut cfg = AppConfig::default_embedded();
    cfg.customer.control.accounts_per_customer = accounts;
    cfg.customer.control.cards_per_account = cards;

    let refs = tmp.join("refs");
    scrip::fixtures::write_reference_fixtures(&refs).unwrap();

    let params = GenParams {
        seed: 3,
        customers: 50,
        days: 30,
        base_end_date: fixed_date(),
        input_dir: refs,
        output_dir: tmp.join("out"),
    };

    generate_all(&cfg, &params).unwrap()
}

#[test]
fn accounts_per_customer_is_honored() {
    let tmp = tempfile::tempdir().unwrap();
    let data = run_with(
        tmp.path(),
        RangeControl { min: 3, max: 3 },
        RangeControl { min: 1, max: 1 },
    );
    assert_eq!(data.accounts.len(), 3 * data.customers.len());
    assert_eq!(data.cards.len(), data.accounts.len());
}

#[test]
fn cards_per_account_is_honored() {
    let tmp = tempfile::tempdir().unwrap();
    let data = run_with(
        tmp.path(),
        RangeControl { min: 2, max: 2 },
        RangeControl { min: 2, max: 2 },
    );
    assert_eq!(data.accounts.len(), 2 * data.customers.len());
    assert_eq!(data.cards.len(), 2 * data.accounts.len());
}
