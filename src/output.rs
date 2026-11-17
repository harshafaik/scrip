//! Write a [`Dataset`] to Parquet files, flat or partitioned.
//!
//! The column sets are the versioned contract in [`crate::schema`]. Every file
//! carries file-level `scrip.*` metadata, and every run writes `_dataset.json`
//! with row counts and SHA-256 hashes.

use crate::config::RangeControl;
use crate::core::writer::write_parquet_with_metadata;
use crate::models::account::Account;
use crate::models::card::Card;
use crate::models::customer::Customer;
use crate::models::dataset::Dataset;
use crate::models::fraud_metadata::FraudMetadata;
use crate::models::transaction::Transaction;
use chrono::{DateTime, NaiveDate, Utc};
use polars::prelude::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// Everything the output layer needs to label a run.
#[derive(Debug, Clone)]
pub struct DatasetMeta {
    pub seed: u64,
    pub base_date: DateTime<Utc>,
    pub days: i64,
    pub customers: usize,
    pub accounts_per_customer: RangeControl,
    pub cards_per_account: RangeControl,
    pub transactions_per_customer: RangeControl,
}

/// Write the five tables as flat `<table>.parquet` files.
pub fn write_dataset(
    ds: &Dataset,
    out_dir: &Path,
    meta: &DatasetMeta,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(out_dir)?;
    let metadata = file_metadata(meta);
    let mut files = BTreeMap::new();

    let mut customers = customers_frame(&ds.customers)?;
    let customers_path = out_dir.join("customers.parquet");
    write_parquet_with_metadata(&mut customers, &customers_path, metadata.clone())?;
    files.insert(
        "customers".to_string(),
        table_entry(customers.height(), sha256_file(&customers_path)?),
    );

    let mut accounts = accounts_frame(&ds.accounts)?;
    let accounts_path = out_dir.join("accounts.parquet");
    write_parquet_with_metadata(&mut accounts, &accounts_path, metadata.clone())?;
    files.insert(
        "accounts".to_string(),
        table_entry(accounts.height(), sha256_file(&accounts_path)?),
    );

    let mut cards = cards_frame(&ds.cards)?;
    let cards_path = out_dir.join("cards.parquet");
    write_parquet_with_metadata(&mut cards, &cards_path, metadata.clone())?;
    files.insert(
        "cards".to_string(),
        table_entry(cards.height(), sha256_file(&cards_path)?),
    );

    let transaction_refs: Vec<&Transaction> = ds.transactions.iter().collect();
    let mut transactions = transactions_frame(&transaction_refs)?;
    let transactions_path = out_dir.join("transactions.parquet");
    write_parquet_with_metadata(&mut transactions, &transactions_path, metadata.clone())?;
    files.insert(
        "transactions".to_string(),
        table_entry(transactions.height(), sha256_file(&transactions_path)?),
    );

    let metadata_refs: Vec<&FraudMetadata> = ds.fraud_metadata.iter().collect();
    let mut fraud_metadata = fraud_metadata_frame(&metadata_refs)?;
    let fraud_metadata_path = out_dir.join("fraud_metadata.parquet");
    write_parquet_with_metadata(&mut fraud_metadata, &fraud_metadata_path, metadata)?;
    files.insert(
        "fraud_metadata".to_string(),
        table_entry(fraud_metadata.height(), sha256_file(&fraud_metadata_path)?),
    );

    write_manifest(out_dir, meta, &files)
}

/// Write the five tables as `event_date=YYYY-MM-DD` partitions.
///
/// Transactions and fraud metadata are partitioned by the transaction date.
/// The snapshot tables are written once, under the run's as-of date. No
/// `event_date` column is added.
pub fn write_dataset_partitioned(
    ds: &Dataset,
    out_dir: &Path,
    meta: &DatasetMeta,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(out_dir)?;
    let metadata = file_metadata(meta);
    let as_of = meta.base_date.date_naive();
    let mut files: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let mut rows: BTreeMap<String, usize> = BTreeMap::new();

    let mut customers = customers_frame(&ds.customers)?;
    write_partition(
        out_dir,
        "customers",
        as_of,
        &mut customers,
        &metadata,
        &mut files,
        &mut rows,
    )?;

    let mut accounts = accounts_frame(&ds.accounts)?;
    write_partition(
        out_dir,
        "accounts",
        as_of,
        &mut accounts,
        &metadata,
        &mut files,
        &mut rows,
    )?;

    let mut cards = cards_frame(&ds.cards)?;
    write_partition(
        out_dir,
        "cards",
        as_of,
        &mut cards,
        &metadata,
        &mut files,
        &mut rows,
    )?;

    let mut by_date: BTreeMap<NaiveDate, Vec<&Transaction>> = BTreeMap::new();
    let mut transaction_dates: HashMap<String, NaiveDate> = HashMap::new();
    for tx in &ds.transactions {
        let date = parse_ts(&tx.timestamp).date_naive();
        by_date.entry(date).or_default().push(tx);
        transaction_dates.insert(tx.transaction_id.clone(), date);
    }
    for (date, transactions) in by_date {
        let mut frame = transactions_frame(&transactions)?;
        write_partition(
            out_dir,
            "transactions",
            date,
            &mut frame,
            &metadata,
            &mut files,
            &mut rows,
        )?;
    }

    let mut metadata_by_date: BTreeMap<NaiveDate, Vec<&FraudMetadata>> = BTreeMap::new();
    for item in &ds.fraud_metadata {
        let date = transaction_dates
            .get(&item.transaction_id)
            .copied()
            .ok_or_else(|| {
                format!(
                    "fraud_metadata references unknown transaction {}",
                    item.transaction_id
                )
            })?;
        metadata_by_date.entry(date).or_default().push(item);
    }
    for (date, items) in metadata_by_date {
        let mut frame = fraud_metadata_frame(&items)?;
        write_partition(
            out_dir,
            "fraud_metadata",
            date,
            &mut frame,
            &metadata,
            &mut files,
            &mut rows,
        )?;
    }

    let mut entries = BTreeMap::new();
    for table in [
        "customers",
        "accounts",
        "cards",
        "transactions",
        "fraud_metadata",
    ] {
        let paths = files.get(table).cloned().unwrap_or_default();
        let row_count = *rows.get(table).unwrap_or(&0);
        entries.insert(
            table.to_string(),
            table_entry(row_count, sha256_files(&paths)?),
        );
    }

    write_manifest(out_dir, meta, &entries)
}

fn write_partition(
    out_dir: &Path,
    table: &str,
    date: NaiveDate,
    frame: &mut DataFrame,
    metadata: &[(String, String)],
    files: &mut BTreeMap<String, Vec<PathBuf>>,
    rows: &mut BTreeMap<String, usize>,
) -> Result<(), Box<dyn Error>> {
    let directory = out_dir
        .join(table)
        .join(format!("event_date={}", date.format("%Y-%m-%d")));
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{table}.parquet"));
    write_parquet_with_metadata(frame, &path, metadata.to_vec())?;
    files.entry(table.to_string()).or_default().push(path);
    *rows.entry(table.to_string()).or_default() += frame.height();
    Ok(())
}

fn file_metadata(meta: &DatasetMeta) -> Vec<(String, String)> {
    vec![
        (
            "scrip.schema_version".to_string(),
            crate::schema::SCHEMA_VERSION.to_string(),
        ),
        (
            "scrip.dataset_version".to_string(),
            crate::schema::DATASET_VERSION.to_string(),
        ),
        ("scrip.seed".to_string(), meta.seed.to_string()),
        (
            "scrip.base_date".to_string(),
            meta.base_date.to_rfc3339(),
        ),
    ]
}

fn write_manifest(
    out_dir: &Path,
    meta: &DatasetMeta,
    files: &BTreeMap<String, Value>,
) -> Result<(), Box<dyn Error>> {
    let mut params = BTreeMap::new();
    params.insert(
        "accounts_per_customer".to_string(),
        range_value(meta.accounts_per_customer),
    );
    params.insert(
        "base_date".to_string(),
        json!(meta.base_date.to_rfc3339()),
    );
    params.insert(
        "cards_per_account".to_string(),
        range_value(meta.cards_per_account),
    );
    params.insert("customers".to_string(), json!(meta.customers));
    params.insert("days".to_string(), json!(meta.days));
    params.insert(
        "transactions_per_customer".to_string(),
        range_value(meta.transactions_per_customer),
    );

    let mut root = BTreeMap::new();
    root.insert(
        "base_date".to_string(),
        json!(meta.base_date.to_rfc3339()),
    );
    root.insert(
        "dataset_version".to_string(),
        json!(crate::schema::DATASET_VERSION),
    );
    root.insert(
        "files".to_string(),
        Value::Object(files.iter().map(|(key, value)| (key.clone(), value.clone())).collect()),
    );
    root.insert(
        "params".to_string(),
        Value::Object(params.into_iter().collect()),
    );
    root.insert(
        "schema_version".to_string(),
        json!(crate::schema::SCHEMA_VERSION),
    );
    root.insert("seed".to_string(), json!(meta.seed));

    let text = serde_json::to_string_pretty(&root)? + "\n";
    let temporary = out_dir.join("_dataset.json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, out_dir.join("_dataset.json"))?;
    Ok(())
}

fn range_value(range: RangeControl) -> Value {
    let mut map = BTreeMap::new();
    map.insert("max".to_string(), json!(range.max));
    map.insert("min".to_string(), json!(range.min));
    Value::Object(map.into_iter().collect())
}

fn table_entry(rows: usize, sha256: String) -> Value {
    let mut map = BTreeMap::new();
    map.insert("rows".to_string(), json!(rows));
    map.insert("sha256".to_string(), json!(sha256));
    Value::Object(map.into_iter().collect())
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let digest = Sha256::digest(fs::read(path)?);
    Ok(hex_digest(digest.as_slice()))
}

fn sha256_files(paths: &[PathBuf]) -> Result<String, Box<dyn Error>> {
    let mut hasher = Sha256::new();
    for path in paths {
        hasher.update(fs::read(path)?);
    }
    Ok(hex_digest(hasher.finalize().as_slice()))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn customers_frame(customers: &[Customer]) -> PolarsResult<DataFrame> {
    df!(
        "customer_id" => customers.iter().map(|c| c.customer_id.clone()).collect::<Vec<_>>(),
        "name" => customers.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "age" => customers.iter().map(|c| c.age as u32).collect::<Vec<_>>(),
        "email" => customers.iter().map(|c| c.email.clone()).collect::<Vec<_>>(),
        "state" => customers.iter().map(|c| c.location.state.clone()).collect::<Vec<_>>(),
        "location" => customers.iter().map(|c| c.location.location.clone()).collect::<Vec<_>>(),
        "location_type" => customers.iter().map(|c| c.location.location_type.clone()).collect::<Vec<_>>(),
        "home_latitude" => customers.iter().map(|c| c.location.home_latitude).collect::<Vec<_>>(),
        "home_longitude" => customers.iter().map(|c| c.location.home_longitude).collect::<Vec<_>>(),
        "home_h3r5" => customers.iter().map(|c| c.location.home_h3r5.clone()).collect::<Vec<_>>(),
        "home_h3r7" => customers.iter().map(|c| c.location.home_h3r7.clone()).collect::<Vec<_>>(),
        "credit_score" => customers.iter().map(|c| c.financial.credit_score as u32).collect::<Vec<_>>(),
        "monthly_spend" => customers.iter().map(|c| c.financial.monthly_spend).collect::<Vec<_>>(),
        "customer_risk_score" => customers.iter().map(|c| c.financial.customer_risk_score as f64).collect::<Vec<_>>(),
        "is_fraud" => customers.iter().map(|c| c.financial.is_fraud).collect::<Vec<_>>(),
        "primary_ua" => customers.iter().map(|c| c.device.primary_ua.clone()).collect::<Vec<_>>(),
        "secondary_ua" => customers.iter().map(|c| c.device.secondary_ua.clone()).collect::<Vec<_>>(),
        "isp" => customers.iter().map(|c| c.device.isp.clone()).collect::<Vec<_>>(),
        "ip_subnet" => customers.iter().map(|c| c.device.ip_subnet.clone()).collect::<Vec<_>>(),
        "registration_date" => customers.iter().map(|c| parse_date(&c.registration_date)).collect::<Vec<_>>()
    )
}

fn accounts_frame(accounts: &[Account]) -> PolarsResult<DataFrame> {
    df!(
        "account_id" => accounts.iter().map(|a| a.account_id.clone()).collect::<Vec<_>>(),
        "customer_id" => accounts.iter().map(|a| a.customer_id.clone()).collect::<Vec<_>>(),
        "bank_id" => accounts.iter().map(|a| a.bank_id.clone()).collect::<Vec<_>>(),
        "account_no" => accounts.iter().map(|a| a.account_no.clone()).collect::<Vec<_>>(),
        "account_type" => accounts.iter().map(|a| a.account_type.clone()).collect::<Vec<_>>(),
        "balance" => accounts.iter().map(|a| a.balance).collect::<Vec<_>>(),
        "status" => accounts.iter().map(|a| a.account_status.clone()).collect::<Vec<_>>(),
        "creation_date" => accounts.iter().map(|a| parse_date(&a.creation_date)).collect::<Vec<_>>()
    )
}

fn cards_frame(cards: &[Card]) -> PolarsResult<DataFrame> {
    df!(
        "card_id" => cards.iter().map(|c| c.card_id.clone()).collect::<Vec<_>>(),
        "account_id" => cards.iter().map(|c| c.account_id.clone()).collect::<Vec<_>>(),
        "customer_id" => cards.iter().map(|c| c.customer_id.clone()).collect::<Vec<_>>(),
        "card_number" => cards.iter().map(|c| c.card_number.clone()).collect::<Vec<_>>(),
        "card_network" => cards.iter().map(|c| c.card_network.clone()).collect::<Vec<_>>(),
        "card_type" => cards.iter().map(|c| c.card_type.clone()).collect::<Vec<_>>(),
        "status" => cards.iter().map(|c| c.status.clone()).collect::<Vec<_>>(),
        "status_reason" => cards.iter().map(|c| c.status_reason.clone()).collect::<Vec<_>>(),
        "issue_date" => cards.iter().map(|c| parse_date(&c.issue_date)).collect::<Vec<_>>(),
        "activation_date" => cards.iter().map(|c| parse_date(&c.activation_date)).collect::<Vec<_>>(),
        "expiry_date" => cards.iter().map(|c| parse_date(&c.expiry_date)).collect::<Vec<_>>(),
        "contactless_limit" => cards.iter().map(|c| c.contactless_limit.clone()).collect::<Vec<_>>(),
        "daily_atm_limit" => cards.iter().map(|c| c.daily_atm_limit.clone()).collect::<Vec<_>>(),
        "online_limit" => cards.iter().map(|c| c.online_limit.clone()).collect::<Vec<_>>(),
        "international_usage" => cards.iter().map(|c| c.international_usage.clone()).collect::<Vec<_>>(),
        "issuing_bank" => cards.iter().map(|c| c.issuing_bank.clone()).collect::<Vec<_>>(),
        "bank_code" => cards.iter().map(|c| c.bank_code.clone()).collect::<Vec<_>>()
    )
}

fn transactions_frame(transactions: &[&Transaction]) -> PolarsResult<DataFrame> {
    let timestamps: Vec<i64> = transactions
        .iter()
        .map(|t| parse_ts(&t.timestamp).timestamp_micros())
        .collect();

    let mut frame = df!(
        "transaction_id" => transactions.iter().map(|t| t.transaction_id.clone()).collect::<Vec<_>>(),
        "card_id" => transactions.iter().map(|t| t.card_id.clone()).collect::<Vec<_>>(),
        "account_id" => transactions.iter().map(|t| t.account_id.clone()).collect::<Vec<_>>(),
        "customer_id" => transactions.iter().map(|t| t.customer_id.clone()).collect::<Vec<_>>(),
        "merchant_id" => transactions.iter().map(|t| t.merchant_id.clone()).collect::<Vec<_>>(),
        "merchant_name" => transactions.iter().map(|t| t.merchant_name.clone()).collect::<Vec<_>>(),
        "merchant_category" => transactions.iter().map(|t| t.merchant_category.clone()).collect::<Vec<_>>(),
        "mcc" => transactions.iter().map(|t| t.mcc.clone()).collect::<Vec<_>>(),
        "merchant_country" => transactions.iter().map(|t| t.merchant_country.clone()).collect::<Vec<_>>(),
        "amount" => transactions.iter().map(|t| t.amount).collect::<Vec<_>>(),
        "currency" => transactions.iter().map(|t| t.currency.clone()).collect::<Vec<_>>(),
        "timestamp" => timestamps,
        "transaction_channel" => transactions.iter().map(|t| t.transaction_channel.clone()).collect::<Vec<_>>(),
        "card_present" => transactions.iter().map(|t| t.card_present).collect::<Vec<_>>(),
        "user_agent" => transactions.iter().map(|t| t.user_agent.clone()).collect::<Vec<_>>(),
        "ip_address" => transactions.iter().map(|t| t.ip_address.clone()).collect::<Vec<_>>(),
        "status" => transactions.iter().map(|t| t.status.clone()).collect::<Vec<_>>(),
        "auth_status" => transactions.iter().map(|t| t.auth_status.clone()).collect::<Vec<_>>(),
        "failure_reason" => transactions.iter().map(|t| t.failure_reason.clone()).collect::<Vec<_>>(),
        "is_fraud" => transactions.iter().map(|t| t.is_fraud).collect::<Vec<_>>(),
        "chargeback" => transactions.iter().map(|t| t.chargeback).collect::<Vec<_>>(),
        "chargeback_days" => transactions.iter().map(|t| t.chargeback_days).collect::<Vec<_>>(),
        "location_lat" => transactions.iter().map(|t| t.location_lat).collect::<Vec<_>>(),
        "location_long" => transactions.iter().map(|t| t.location_long).collect::<Vec<_>>(),
        "h3_r7" => transactions.iter().map(|t| t.h3_r7.clone()).collect::<Vec<_>>()
    )?;

    let timestamp = frame
        .column("timestamp")?
        .as_materialized_series()
        .i64()?
        .clone()
        .into_datetime(TimeUnit::Microseconds, Some(TimeZone::UTC))
        .into_series();
    frame.with_column(timestamp)?;

    Ok(frame)
}

fn fraud_metadata_frame(metadata: &[&FraudMetadata]) -> PolarsResult<DataFrame> {
    df!(
        "transaction_id" => metadata.iter().map(|m| m.transaction_id.clone()).collect::<Vec<_>>(),
        "fraud_target" => metadata.iter().map(|m| m.fraud_target).collect::<Vec<_>>(),
        "fraud_type" => metadata.iter().map(|m| m.fraud_type.clone()).collect::<Vec<_>>(),
        "label_noise" => metadata.iter().map(|m| m.label_noise.clone()).collect::<Vec<_>>(),
        "injector_version" => metadata.iter().map(|m| m.injector_version.clone()).collect::<Vec<_>>(),
        "geo_anomaly" => metadata.iter().map(|m| m.geo_anomaly).collect::<Vec<_>>(),
        "device_anomaly" => metadata.iter().map(|m| m.device_anomaly).collect::<Vec<_>>(),
        "ip_anomaly" => metadata.iter().map(|m| m.ip_anomaly).collect::<Vec<_>>(),
        "flags" => metadata.iter().map(|m| m.flags.as_ref().map(|f| f.join(","))).collect::<Vec<_>>(),
        "burst_session" => metadata.iter().map(|m| m.burst_session).collect::<Vec<_>>(),
        "burst_seq" => metadata.iter().map(|m| m.burst_seq).collect::<Vec<_>>(),
        "campaign_id" => metadata.iter().map(|m| m.campaign_id.clone()).collect::<Vec<_>>(),
        "campaign_type" => metadata.iter().map(|m| m.campaign_type.clone()).collect::<Vec<_>>(),
        "campaign_phase" => metadata.iter().map(|m| m.campaign_phase.clone()).collect::<Vec<_>>(),
        "campaign_day_number" => metadata.iter().map(|m| m.campaign_day_number).collect::<Vec<_>>()
    )
}

fn parse_date(raw: &str) -> NaiveDate {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .unwrap_or_else(|error| panic!("invalid date {raw:?}: {error}"))
}

fn parse_ts(raw: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .unwrap_or_else(|error| panic!("invalid timestamp {raw:?}: {error}"))
        .with_timezone(&Utc)
}
