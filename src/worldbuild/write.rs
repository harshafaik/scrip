//! Write enriched rows to the two reference Parquet files.

use crate::core::writer::write_parquet;
use polars::prelude::*;
use std::error::Error;
use std::path::Path;

pub struct ResidentialRow {
    pub osm_id: i64,
    pub h3_index: String,
    pub latitude: f64,
    pub longitude: f64,
    pub city: Option<String>,
    pub pincode: Option<String>,
    pub state: String,
    pub district_name: Option<String>,
}

pub struct MerchantRow {
    pub osm_id: i64,
    pub h3_index: String,
    pub merchant_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub merchant_category: String,
    pub risk_level: String,
    pub city: Option<String>,
    pub pincode: Option<String>,
    pub state: String,
    pub district_name: Option<String>,
}

pub fn write(
    residential: &[ResidentialRow],
    merchants: &[MerchantRow],
    out_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(out_dir)?;

    let mut residential_df = df!(
        "osm_id" => residential.iter().map(|r| r.osm_id).collect::<Vec<_>>(),
        "h3_index" => residential.iter().map(|r| r.h3_index.clone()).collect::<Vec<_>>(),
        "latitude" => residential.iter().map(|r| r.latitude).collect::<Vec<_>>(),
        "longitude" => residential.iter().map(|r| r.longitude).collect::<Vec<_>>(),
        "city" => residential.iter().map(|r| r.city.clone()).collect::<Vec<_>>(),
        "pincode" => residential.iter().map(|r| r.pincode.clone()).collect::<Vec<_>>(),
        "state" => residential.iter().map(|r| r.state.clone()).collect::<Vec<_>>(),
        "district_name" => residential.iter().map(|r| r.district_name.clone()).collect::<Vec<_>>()
    )?;
    write_parquet(
        &mut residential_df,
        &out_dir.join("ref_residential.parquet"),
    )?;

    let mut merchant_df = df!(
        "osm_id" => merchants.iter().map(|m| m.osm_id).collect::<Vec<_>>(),
        "h3_index" => merchants.iter().map(|m| m.h3_index.clone()).collect::<Vec<_>>(),
        "latitude" => merchants.iter().map(|m| m.latitude).collect::<Vec<_>>(),
        "longitude" => merchants.iter().map(|m| m.longitude).collect::<Vec<_>>(),
        "merchant_name" => merchants.iter().map(|m| m.merchant_name.clone()).collect::<Vec<_>>(),
        "merchant_category" => merchants.iter().map(|m| m.merchant_category.clone()).collect::<Vec<_>>(),
        "risk_level" => merchants.iter().map(|m| m.risk_level.clone()).collect::<Vec<_>>(),
        "city" => merchants.iter().map(|m| m.city.clone()).collect::<Vec<_>>(),
        "pincode" => merchants.iter().map(|m| m.pincode.clone()).collect::<Vec<_>>(),
        "state" => merchants.iter().map(|m| m.state.clone()).collect::<Vec<_>>(),
        "district_name" => merchants.iter().map(|m| m.district_name.clone()).collect::<Vec<_>>()
    )?;
    write_parquet(&mut merchant_df, &out_dir.join("ref_merchants.parquet"))?;

    Ok(())
}
