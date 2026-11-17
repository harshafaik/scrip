//! Manual fidelity report (T0/T8).
//!
//! Run with:
//!     cargo test --test metrics_report -- --ignored --nocapture

use chrono::{DateTime, Utc};
use scrip::config::AppConfig;
use scrip::fixtures;
use scrip::generators::run::{generate_all, GenParams};

#[test]
#[ignore = "manual fidelity report; run with --ignored --nocapture"]
fn fidelity_report() {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    fixtures::write_reference_fixtures(&refs).unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let params = GenParams {
        seed: 42,
        customers: 25,
        days: 30,
        base_end_date: base,
        input_dir: refs,
        output_dir: tmp.path().join("out"),
    };

    let data = generate_all(&AppConfig::default_embedded(), &params).unwrap();
    let report = scrip::metrics::audit_dataset(&data);
    scrip::metrics::print_report(&report);
}
