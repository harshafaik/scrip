use chrono::{DateTime, Utc};
use scrip::config::AppConfig;
use scrip::generators::run::{generate_all, GenParams};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

fn fingerprint() -> u64 {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    scrip::fixtures::write_reference_fixtures(&refs).unwrap();

    let cfg = AppConfig::default_embedded();
    let params = GenParams {
        seed: 42,
        customers: 25,
        days: 30,
        base_end_date: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        input_dir: refs,
        output_dir: tmp.path().join("out"),
    };

    let data = generate_all(&cfg, &params).unwrap();
    let mut h = DefaultHasher::new();
    format!("{data:?}").hash(&mut h);
    h.finish()
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden.txt")
}

fn read_golden() -> u64 {
    fs::read_to_string(golden_path())
        .expect("tests/golden.txt is missing; run SCRIP_REGEN_GOLDEN=1 cargo test golden")
        .trim()
        .parse()
        .expect("tests/golden.txt is not a u64")
}

fn write_golden(value: u64) {
    let path = golden_path();
    let temporary = path.with_extension("txt.tmp");
    fs::write(&temporary, format!("{value}\n")).unwrap();
    fs::rename(temporary, path).unwrap();
}

#[test]
fn dataset_fingerprint_is_stable() {
    let got = fingerprint();
    println!("golden fingerprint = {got}");

    if std::env::var("SCRIP_REGEN_GOLDEN").as_deref() == Ok("1") {
        write_golden(got);
        println!("wrote {}", golden_path().display());
        return;
    }

    assert_eq!(got, read_golden(), "observed fingerprint = {got}");
}
