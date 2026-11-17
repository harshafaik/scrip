use chrono::{DateTime, Utc};
use scrip::config::AppConfig;
use scrip::fixtures;
use scrip::generators::run::{generate_all, GenParams};
use scrip::metrics;
use scrip::output::{write_dataset, DatasetMeta};

fn fixture() -> (tempfile::TempDir, scrip::Dataset, DatasetMeta) {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    fixtures::write_reference_fixtures(&refs).unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let params = GenParams {
        seed: 11,
        customers: 40,
        days: 30,
        base_end_date: base,
        input_dir: refs,
        output_dir: tmp.path().join("out"),
    };
    let cfg = AppConfig::default_embedded();
    let meta = DatasetMeta {
        seed: params.seed,
        base_date: params.base_end_date,
        days: params.days,
        customers: params.customers,
        accounts_per_customer: cfg.customer.control.accounts_per_customer,
        cards_per_account: cfg.customer.control.cards_per_account,
        transactions_per_customer: cfg.customer.control.transactions_per_customer,
    };

    let data = generate_all(&cfg, &params).unwrap();
    (tmp, data, meta)
}

#[test]
fn audit_reports_both_classes_and_reads_output_dir() {
    let (tmp, data, meta) = fixture();
    let report = metrics::audit_dataset(&data);

    assert_eq!(report.statistical.transactions, data.transactions.len());
    assert!(report.statistical.fraud_target_rate > 0.0);
    assert!(report.statistical.fraud_target_rate < 100.0);
    assert!(report.behavioral.velocity_3_1h.fraud.is_finite());
    assert!(report.behavioral.velocity_3_1h.legit.is_finite());
    assert!(report.behavioral.velocity_5_24h.fraud.is_finite());
    assert!(report.behavioral.velocity_5_24h.legit.is_finite());
    assert!(!report.leakage.features.is_empty());

    let out = tmp.path().join("out");
    write_dataset(&data, &out, &meta).unwrap();
    let from_dir = metrics::audit_dir(&out).unwrap();

    assert_eq!(from_dir.statistical.transactions, data.transactions.len());
    assert_eq!(from_dir.statistical.customers, data.customers.len());
    assert!(!from_dir.leakage.features.is_empty());
}
