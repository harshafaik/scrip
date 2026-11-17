use chrono::{DateTime, Utc};
use polars::prelude::*;
use scrip::config::AppConfig;
use scrip::fixtures;
use scrip::generators::run::{generate_all, GenParams};
use scrip::output::{write_dataset, DatasetMeta};
use scrip::schema;
use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;

fn frame(path: &Path) -> DataFrame {
    ParquetReader::new(File::open(path).unwrap())
        .finish()
        .unwrap()
}

fn columns(path: &Path) -> Vec<String> {
    frame(path)
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn dtype(path: &Path, column: &str) -> DataType {
    frame(path)
        .schema()
        .get(column)
        .cloned()
        .unwrap_or_else(|| panic!("{} has no column {column}", path.display()))
}

fn kv_metadata(path: &Path) -> BTreeMap<String, String> {
    let mut reader = ParquetReader::new(File::open(path).unwrap());
    let metadata = reader.get_metadata().unwrap();
    metadata
        .key_value_metadata()
        .as_ref()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| Some((item.key.clone(), item.value.clone()?)))
                .collect()
        })
        .unwrap_or_default()
}

fn meta(cfg: &AppConfig, params: &GenParams) -> DatasetMeta {
    DatasetMeta {
        seed: params.seed,
        base_date: params.base_end_date,
        days: params.days,
        customers: params.customers,
        accounts_per_customer: cfg.customer.control.accounts_per_customer,
        cards_per_account: cfg.customer.control.cards_per_account,
        transactions_per_customer: cfg.customer.control.transactions_per_customer,
    }
}

#[test]
fn generated_tables_match_the_published_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    fixtures::write_reference_fixtures(&refs).unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let params = GenParams::new(42, 10, 30)
        .base_end_date(base)
        .input_dir(&refs)
        .output_dir(tmp.path().join("out"));
    let cfg = AppConfig::default_embedded();

    let data = generate_all(&cfg, &params).unwrap();
    write_dataset(&data, &params.output_dir, &meta(&cfg, &params)).unwrap();

    let marker = format!("dataset={}", schema::DATASET_VERSION);
    assert!(
        !data.fraud_metadata.is_empty(),
        "fixture produced no fraud metadata"
    );
    assert!(
        data.fraud_metadata
            .iter()
            .all(|m| m.injector_version.contains(&marker)),
        "injector_version is missing {marker}"
    );

    for &(file, expected) in schema::TABLES {
        assert_eq!(
            columns(&params.output_dir.join(file)),
            expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "{file} drifted from scrip::schema"
        );
    }

    let datetime = DataType::Datetime(TimeUnit::Microseconds, Some(TimeZone::UTC));
    assert_eq!(
        dtype(&params.output_dir.join("transactions.parquet"), "timestamp"),
        datetime
    );
    for &(table, column) in schema::DATE_COLUMNS {
        assert_eq!(
            dtype(&params.output_dir.join(format!("{table}.parquet")), column),
            DataType::Date,
            "{table}.{column} is not Date"
        );
    }

    let base_date = params.base_end_date.to_rfc3339();
    for &(file, _) in schema::TABLES {
        let metadata = kv_metadata(&params.output_dir.join(file));
        assert_eq!(
            metadata.get("scrip.schema_version").map(String::as_str),
            Some("2"),
            "{file} schema metadata"
        );
        assert_eq!(
            metadata.get("scrip.dataset_version").map(String::as_str),
            Some(schema::DATASET_VERSION),
            "{file} dataset metadata"
        );
        assert_eq!(
            metadata.get("scrip.seed").map(String::as_str),
            Some("42"),
            "{file} seed metadata"
        );
        assert_eq!(
            metadata.get("scrip.base_date").map(String::as_str),
            Some(base_date.as_str()),
            "{file} base_date metadata"
        );
    }
}
