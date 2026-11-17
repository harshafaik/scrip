use chrono::{DateTime, Utc};
use scrip::config::AppConfig;
use scrip::generators::run::{generate_all, GenParams};
use std::path::Path;

fn fixed_date() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn run(dir: &Path, seed: u64) -> String {
    let refs = dir.join("refs");
    scrip::fixtures::write_reference_fixtures(&refs).unwrap();

    let cfg = AppConfig::default_embedded();
    let params = GenParams {
        seed,
        customers: 25,
        days: 30,
        base_end_date: fixed_date(),
        input_dir: refs,
        output_dir: dir.join("out"),
    };

    let data = generate_all(&cfg, &params).unwrap();
    format!("{data:?}")
}

#[test]
fn identical_seed_and_params_are_identical() {
    let tmp = tempfile::tempdir().unwrap();
    let a = run(tmp.path(), 42);
    let b = run(tmp.path(), 42);
    assert_eq!(a, b, "same seed + params produced different output");
}

#[test]
fn different_seed_differs() {
    let tmp = tempfile::tempdir().unwrap();
    let a = run(tmp.path(), 1);
    let b = run(tmp.path(), 2);
    assert_ne!(a, b, "different seeds produced identical output");
}
