//! P0 acceptance: partitioned layout, as-of labels, and the dataset manifest.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use polars::prelude::*;
use scrip::config::AppConfig;
use scrip::fixtures;
use scrip::generators::run::{generate_all, GenParams};
use scrip::output::{write_dataset, write_dataset_partitioned, DatasetMeta};
use scrip::schema;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const TABLES: [&str; 5] = [
    "customers",
    "accounts",
    "cards",
    "transactions",
    "fraud_metadata",
];

fn generate(
    customers: usize,
    days: i64,
    seed: u64,
) -> (tempfile::TempDir, scrip::Dataset, DatasetMeta) {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    fixtures::write_reference_fixtures(&refs).unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-31T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let params = GenParams {
        seed,
        customers,
        days,
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

fn frame(path: &Path) -> DataFrame {
    ParquetReader::new(fs::File::open(path).unwrap())
        .finish()
        .unwrap()
}

fn partition_files(root: &Path, table: &str) -> Vec<PathBuf> {
    let table_root = root.join(table);
    if !table_root.exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&table_root).unwrap() {
        let directory = entry.unwrap().path();
        if !directory.is_dir() {
            continue;
        }
        let file = directory.join(format!("{table}.parquet"));
        if file.exists() {
            files.push(file);
        }
    }
    files.sort();
    files
}

fn event_date(path: &Path) -> NaiveDate {
    let directory = path.parent().unwrap().file_name().unwrap().to_str().unwrap();
    let raw = directory.strip_prefix("event_date=").unwrap();
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").unwrap()
}

fn sha256_file(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(fs::read(path).unwrap());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn snapshots_and_transactions_respect_the_run_window() {
    let (_tmp, data, meta) = generate(200, 30, 42);
    let as_of = meta.base_date.date_naive();

    for customer in &data.customers {
        assert!(
            NaiveDate::parse_from_str(&customer.registration_date, "%Y-%m-%d").unwrap() <= as_of,
            "registration {} is after as-of {as_of}",
            customer.registration_date
        );
    }

    for card in &data.cards {
        assert!(
            NaiveDate::parse_from_str(&card.issue_date, "%Y-%m-%d").unwrap() <= as_of,
            "issue {} is after as-of {as_of}",
            card.issue_date
        );
        assert!(
            NaiveDate::parse_from_str(&card.activation_date, "%Y-%m-%d").unwrap() <= as_of,
            "activation {} is after as-of {as_of}",
            card.activation_date
        );
    }

    let start = meta.base_date - Duration::days(meta.days);
    for tx in &data.transactions {
        let ts = DateTime::parse_from_rfc3339(&tx.timestamp)
            .unwrap()
            .with_timezone(&Utc);
        assert!(
            ts >= start && ts < meta.base_date,
            "transaction {ts} outside [{start}, {})",
            meta.base_date
        );
    }
}

#[test]
fn partitioned_output_is_lossless_and_labelled() {
    let (tmp, data, meta) = generate(100, 30, 42);
    let flat = tmp.path().join("flat");
    let partitioned = tmp.path().join("partitioned");
    write_dataset(&data, &flat, &meta).unwrap();
    write_dataset_partitioned(&data, &partitioned, &meta).unwrap();

    for table in TABLES {
        let expected = frame(&flat.join(format!("{table}.parquet"))).height();
        let actual: usize = partition_files(&partitioned, table)
            .iter()
            .map(|path| frame(path).height())
            .sum();
        assert_eq!(
            actual, expected,
            "{table} partition rows do not match flat rows"
        );
    }

    let mut flat_ids: Vec<String> = frame(&flat.join("transactions.parquet"))
        .column("transaction_id")
        .unwrap()
        .str()
        .unwrap()
        .into_no_null_iter()
        .map(String::from)
        .collect();
    let mut partitioned_ids: Vec<String> = partition_files(&partitioned, "transactions")
        .iter()
        .flat_map(|path| {
            frame(path)
                .column("transaction_id")
                .unwrap()
                .str()
                .unwrap()
                .into_no_null_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect();

    assert_eq!(
        partitioned_ids.iter().collect::<HashSet<_>>().len(),
        partitioned_ids.len(),
        "a transaction appears in more than one partition"
    );
    flat_ids.sort();
    partitioned_ids.sort();
    assert_eq!(partitioned_ids, flat_ids);

    for table in TABLES {
        for path in partition_files(&partitioned, table) {
            let date = event_date(&path);
            assert!(
                !frame(&path)
                    .get_column_names()
                    .iter()
                    .any(|column| column.as_str() == "event_date"),
                "{} contains an event_date column",
                path.display()
            );
            if table == "customers" || table == "accounts" || table == "cards" {
                assert_eq!(
                    date,
                    meta.base_date.date_naive(),
                    "{table} snapshot label"
                );
            }
        }
    }
}

#[test]
fn manifest_is_deterministic_and_matches_files() {
    let (tmp, data, meta) = generate(40, 30, 42);
    let first = tmp.path().join("first");
    let second = tmp.path().join("second");
    write_dataset(&data, &first, &meta).unwrap();
    write_dataset(&data, &second, &meta).unwrap();

    let first_text = fs::read_to_string(first.join("_dataset.json")).unwrap();
    let second_text = fs::read_to_string(second.join("_dataset.json")).unwrap();
    assert_eq!(first_text, second_text, "manifest is not deterministic");

    let manifest: serde_json::Value = serde_json::from_str(&first_text).unwrap();
    let base_date = meta.base_date.to_rfc3339();
    assert_eq!(
        manifest["schema_version"].as_u64(),
        Some(schema::SCHEMA_VERSION as u64)
    );
    assert_eq!(
        manifest["dataset_version"].as_str(),
        Some(schema::DATASET_VERSION)
    );
    assert_eq!(manifest["seed"].as_u64(), Some(42));
    assert_eq!(manifest["base_date"].as_str(), Some(base_date.as_str()));

    for table in TABLES {
        let path = first.join(format!("{table}.parquet"));
        let rows = manifest["files"][table]["rows"].as_u64().unwrap() as usize;
        let digest = manifest["files"][table]["sha256"].as_str().unwrap();
        assert_eq!(rows, frame(&path).height(), "{table} manifest rows");
        assert_eq!(digest, sha256_file(&path), "{table} manifest hash");
    }
}
