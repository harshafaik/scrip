//! Reference-data contract for `scrip`.
//!
//! `scrip` does not synthesise geography. It consumes two reference Parquet
//! tables produced by an external world-builder pipeline (e.g. an OSM/PostGIS
//! extraction):
//!
//! * `ref_residential.parquet` — residential points used to place customers.
//! * `ref_merchants.parquet`   — merchant points used to place transactions.
//!
//! This module is the single source of truth for that contract: the versioned
//! schema and the validated loaders the generators use. A reference directory
//! that does not match the schema fails with a descriptive error instead of a
//! panic. Use [`validate`] (the `validate-refs` CLI subcommand) to check a
//! directory before running a generation.

use crate::core::parquet::{
    check_columns, ensure_no_nulls, ensure_non_empty, float_column, int_column, read_parquet,
    string_column,
};
use polars::prelude::DataFrame;
use std::path::Path;

pub use crate::core::parquet::ReferenceError;

/// Version of the reference schema this build expects. Bump whenever the
/// required tables or columns change, and keep the world builder in sync.
pub const REFERENCE_SCHEMA_VERSION: u32 = 1;

/// File name of the residential reference table.
pub const RESIDENTIAL_FILE: &str = "ref_residential.parquet";

/// File name of the merchant reference table.
pub const MERCHANTS_FILE: &str = "ref_merchants.parquet";

const RESIDENTIAL_COLUMNS: &[&str] = &[
    "h3_index",
    "latitude",
    "longitude",
    "state",
    "city",
    "pincode",
];

const MERCHANT_COLUMNS: &[&str] = &[
    "h3_index",
    "merchant_name",
    "latitude",
    "longitude",
    "merchant_category",
    "osm_id",
    "state",
];

/// Residential reference points (`ref_residential.parquet`).
#[derive(Debug, Clone, Default)]
pub struct ResidentialRefs {
    pub h3_index: Vec<String>,
    pub latitude: Vec<f64>,
    pub longitude: Vec<f64>,
    pub state: Vec<String>,
    pub city: Vec<Option<String>>,
    pub pincode: Vec<Option<String>>,
}

/// Merchant reference points (`ref_merchants.parquet`).
#[derive(Debug, Clone, Default)]
pub struct Merchants {
    pub h3_index: Vec<String>,
    pub merchant_name: Vec<String>,
    pub latitude: Vec<f64>,
    pub longitude: Vec<f64>,
    pub merchant_category: Vec<String>,
    pub osm_id: Vec<i64>,
    pub state: Vec<String>,
}

/// A validated summary of a reference directory.
#[derive(Debug, Clone)]
pub struct ReferenceReport {
    pub schema_version: u32,
    pub residential_rows: usize,
    pub merchant_rows: usize,
    pub residential_columns: Vec<String>,
    pub merchant_columns: Vec<String>,
}

/// Load and validate `ref_residential.parquet`.
pub fn load_residential(path: &Path) -> Result<ResidentialRefs, ReferenceError> {
    const TABLE: &str = RESIDENTIAL_FILE;
    let df = read_parquet(path)?;
    check_columns(&df, TABLE, RESIDENTIAL_COLUMNS)?;
    ensure_non_empty(&df, TABLE)?;
    ensure_no_nulls(&df, TABLE, &["h3_index", "latitude", "longitude"])?;

    Ok(ResidentialRefs {
        h3_index: string_column(&df, TABLE, "h3_index")?
            .into_no_null_iter()
            .map(String::from)
            .collect(),
        latitude: float_column(&df, TABLE, "latitude")?
            .into_no_null_iter()
            .collect(),
        longitude: float_column(&df, TABLE, "longitude")?
            .into_no_null_iter()
            .collect(),
        state: state_column(&df, TABLE)?,
        city: string_column(&df, TABLE, "city")?
            .into_iter()
            .map(|o| o.map(String::from))
            .collect(),
        pincode: string_column(&df, TABLE, "pincode")?
            .into_iter()
            .map(|o| o.map(String::from))
            .collect(),
    })
}

/// Load and validate `ref_merchants.parquet`.
pub fn load_merchants(path: &Path) -> Result<Merchants, ReferenceError> {
    const TABLE: &str = MERCHANTS_FILE;
    let df = read_parquet(path)?;
    check_columns(&df, TABLE, MERCHANT_COLUMNS)?;
    ensure_non_empty(&df, TABLE)?;
    ensure_no_nulls(
        &df,
        TABLE,
        &[
            "h3_index",
            "merchant_name",
            "latitude",
            "longitude",
            "merchant_category",
            "osm_id",
        ],
    )?;

    Ok(Merchants {
        h3_index: string_column(&df, TABLE, "h3_index")?
            .into_no_null_iter()
            .map(String::from)
            .collect(),
        merchant_name: string_column(&df, TABLE, "merchant_name")?
            .into_no_null_iter()
            .map(String::from)
            .collect(),
        latitude: float_column(&df, TABLE, "latitude")?
            .into_no_null_iter()
            .collect(),
        longitude: float_column(&df, TABLE, "longitude")?
            .into_no_null_iter()
            .collect(),
        merchant_category: string_column(&df, TABLE, "merchant_category")?
            .into_no_null_iter()
            .map(String::from)
            .collect(),
        osm_id: int_column(&df, TABLE, "osm_id")?
            .into_no_null_iter()
            .collect(),
        state: state_column(&df, TABLE)?,
    })
}

/// Read the `state` column, filling nulls with `"Unknown"`.
///
/// RiskFabric's dbt marts leave `state` null for points outside every boundary
/// polygon. Filling preserves row alignment; `into_no_null_iter()` would skip
/// those rows and misalign every other column.
fn state_column(df: &DataFrame, table: &str) -> Result<Vec<String>, ReferenceError> {
    Ok(string_column(df, table, "state")?
        .into_iter()
        .map(|value| {
            value
                .map(String::from)
                .unwrap_or_else(|| "Unknown".to_string())
        })
        .collect())
}

/// Validate a reference directory against the schema without generating data.
///
/// Both `ref_residential.parquet` and `ref_merchants.parquet` must be present,
/// readable, contain the required columns, and hold at least one row.
pub fn validate(dir: &Path) -> Result<ReferenceReport, ReferenceError> {
    let residential = load_residential(&dir.join(RESIDENTIAL_FILE))?;
    let merchants = load_merchants(&dir.join(MERCHANTS_FILE))?;

    Ok(ReferenceReport {
        schema_version: REFERENCE_SCHEMA_VERSION,
        residential_rows: residential.h3_index.len(),
        merchant_rows: merchants.h3_index.len(),
        residential_columns: RESIDENTIAL_COLUMNS
            .iter()
            .copied()
            .map(String::from)
            .collect(),
        merchant_columns: MERCHANT_COLUMNS.iter().copied().map(String::from).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;
    use std::fs::File;

    #[test]
    fn null_state_is_filled_without_skipping_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(RESIDENTIAL_FILE);

        let mut df = df!(
            "h3_index" => vec!["a".to_string(), "b".to_string()],
            "latitude" => vec![1.0_f64, 2.0],
            "longitude" => vec![3.0_f64, 4.0],
            "state" => vec![Some("Maharashtra".to_string()), None],
            "city" => vec![None::<String>, None],
            "pincode" => vec![None::<String>, None]
        )
        .unwrap();

        let mut file = File::create(&path).unwrap();
        ParquetWriter::new(&mut file).finish(&mut df).unwrap();

        let refs = load_residential(&path).unwrap();
        assert_eq!(refs.h3_index.len(), 2);
        assert_eq!(
            refs.state,
            vec!["Maharashtra".to_string(), "Unknown".to_string()]
        );
    }
}
