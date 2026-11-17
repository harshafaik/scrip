//! Versioned Parquet output contract.
//!
//! These column lists are the boundary between `scrip` and downstream consumers
//! such as RiskFabric's ETL and ML pipeline. They match RiskFabric's `generate`
//! output exactly. Add columns deliberately and bump [`SCHEMA_VERSION`].

/// Version of the generated Parquet schema.
///
/// Version 2 changed `timestamp` to `Datetime(us, UTC)` and the date columns to
/// `Date`.
pub const SCHEMA_VERSION: u32 = 2;

/// Version of the generator's behavior, independent of the Parquet schema.
/// Emitted in `fraud_metadata.injector_version` so consumers can tell which
/// behavior produced a file.
pub const DATASET_VERSION: &str = "2";

/// Date columns, keyed by table name.
pub const DATE_COLUMNS: &[(&str, &str)] = &[
    ("customers", "registration_date"),
    ("accounts", "creation_date"),
    ("cards", "issue_date"),
    ("cards", "activation_date"),
    ("cards", "expiry_date"),
];

/// Timestamp columns, keyed by table name.
pub const TIMESTAMP_COLUMNS: &[(&str, &str)] = &[("transactions", "timestamp")];

/// `customers.parquet` columns, in write order.
pub const CUSTOMERS: &[&str] = &[
    "customer_id",
    "name",
    "age",
    "email",
    "state",
    "location",
    "location_type",
    "home_latitude",
    "home_longitude",
    "home_h3r5",
    "home_h3r7",
    "credit_score",
    "monthly_spend",
    "customer_risk_score",
    "is_fraud",
    "primary_ua",
    "secondary_ua",
    "isp",
    "ip_subnet",
    "registration_date",
];

/// `accounts.parquet` columns, in write order.
pub const ACCOUNTS: &[&str] = &[
    "account_id",
    "customer_id",
    "bank_id",
    "account_no",
    "account_type",
    "balance",
    "status",
    "creation_date",
];

/// `cards.parquet` columns, in write order.
pub const CARDS: &[&str] = &[
    "card_id",
    "account_id",
    "customer_id",
    "card_number",
    "card_network",
    "card_type",
    "status",
    "status_reason",
    "issue_date",
    "activation_date",
    "expiry_date",
    "contactless_limit",
    "daily_atm_limit",
    "online_limit",
    "international_usage",
    "issuing_bank",
    "bank_code",
];

/// `transactions.parquet` columns, in write order.
pub const TRANSACTIONS: &[&str] = &[
    "transaction_id",
    "card_id",
    "account_id",
    "customer_id",
    "merchant_id",
    "merchant_name",
    "merchant_category",
    "mcc",
    "merchant_country",
    "amount",
    "currency",
    "timestamp",
    "transaction_channel",
    "card_present",
    "user_agent",
    "ip_address",
    "status",
    "auth_status",
    "failure_reason",
    "is_fraud",
    "chargeback",
    "chargeback_days",
    "location_lat",
    "location_long",
    "h3_r7",
];

/// `fraud_metadata.parquet` columns, in write order.
pub const FRAUD_METADATA: &[&str] = &[
    "transaction_id",
    "fraud_target",
    "fraud_type",
    "label_noise",
    "injector_version",
    "geo_anomaly",
    "device_anomaly",
    "ip_anomaly",
    "flags",
    "burst_session",
    "burst_seq",
    "campaign_id",
    "campaign_type",
    "campaign_phase",
    "campaign_day_number",
];

/// Every output table and its contract, keyed by file name.
pub const TABLES: &[(&str, &[&str])] = &[
    ("customers.parquet", CUSTOMERS),
    ("accounts.parquet", ACCOUNTS),
    ("cards.parquet", CARDS),
    ("transactions.parquet", TRANSACTIONS),
    ("fraud_metadata.parquet", FRAUD_METADATA),
];
