//! Synthesise a tiny geographic reference dataset so `scrip` can run without the
//! full OSM extraction pipeline. Writes `ref_residential.parquet` and
//! `ref_merchants.parquet` with valid H3 cells.
//!
//! This is a convenience for demos and tests only — the production path consumes
//! real reference Parquets produced by the geospatial pipeline.

use h3o::{LatLng, Resolution};
use polars::prelude::*;
use std::fs::File;
use std::path::Path;

/// (city, state, lat, lon, pincode)
const CITIES: &[(&str, &str, f64, f64, &str)] = &[
    ("Mumbai", "Maharashtra", 19.0760, 72.8777, "400001"),
    ("Delhi", "Delhi", 28.6139, 77.2090, "110001"),
    ("Bengaluru", "Karnataka", 12.9716, 77.5946, "560001"),
    ("Hyderabad", "Telangana", 17.3850, 78.4867, "500001"),
    ("Chennai", "Tamil Nadu", 13.0827, 80.2707, "600001"),
    ("Kolkata", "West Bengal", 22.5726, 88.3639, "700001"),
    ("Pune", "Maharashtra", 18.5204, 73.8567, "411001"),
    ("Ahmedabad", "Gujarat", 23.0225, 72.5714, "380001"),
    ("Jaipur", "Rajasthan", 26.9124, 75.7873, "302001"),
    ("Lucknow", "Uttar Pradesh", 26.8467, 80.9462, "226001"),
    ("Bhopal", "Madhya Pradesh", 23.2599, 77.4126, "462001"),
    ("Kochi", "Kerala", 9.9312, 76.2673, "682001"),
];

/// (category, mcc)
const CATEGORIES: &[(&str, &str)] = &[
    ("GROCERY", "5411"),
    ("FOOD_AND_DRINK", "5812"),
    ("GENERAL_RETAIL", "5311"),
    ("SERVICES", "7299"),
    ("ENTERTAINMENT", "7999"),
    ("ELECTRONICS", "5732"),
    ("TRAVEL", "4722"),
    ("MEDICAL", "8099"),
    ("TRANSPORT", "4111"),
    ("RETAIL", "5999"),
];

fn cell(lat: f64, lon: f64) -> String {
    LatLng::new(lat, lon)
        .expect("valid coords")
        .to_cell(Resolution::Eight)
        .to_string()
}

/// Write the two reference Parquets into `dir`.
pub fn write_reference_fixtures(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;

    // Residential: 4 points per city.
    let mut r_osm: Vec<i64> = Vec::new();
    let mut r_h3: Vec<String> = Vec::new();
    let mut r_lat: Vec<f64> = Vec::new();
    let mut r_lon: Vec<f64> = Vec::new();
    let mut r_city: Vec<String> = Vec::new();
    let mut r_pin: Vec<String> = Vec::new();
    let mut r_state: Vec<String> = Vec::new();

    // Merchants: one per category per city.
    let mut m_osm: Vec<i64> = Vec::new();
    let mut m_h3: Vec<String> = Vec::new();
    let mut m_name: Vec<String> = Vec::new();
    let mut m_lat: Vec<f64> = Vec::new();
    let mut m_lon: Vec<f64> = Vec::new();
    let mut m_cat: Vec<String> = Vec::new();
    let mut m_risk: Vec<String> = Vec::new();
    let mut m_city: Vec<String> = Vec::new();
    let mut m_pin: Vec<String> = Vec::new();
    let mut m_state: Vec<String> = Vec::new();

    let mut osm: i64 = 1;
    for (city, state, lat, lon, pin) in CITIES {
        for k in 0..4 {
            let dlat = (k as f64 - 1.5) * 0.01;
            let dlon = (k as f64 - 1.5) * 0.01;
            let (la, lo) = (lat + dlat, lon + dlon);
            r_osm.push(osm);
            r_h3.push(cell(la, lo));
            r_lat.push(la);
            r_lon.push(lo);
            r_city.push((*city).to_string());
            r_pin.push((*pin).to_string());
            r_state.push((*state).to_string());
            osm += 1;
        }
        for (cat, _mcc) in CATEGORIES {
            let la = lat + 0.005;
            let lo = lon + 0.005;
            m_osm.push(osm);
            m_h3.push(cell(la, lo));
            m_name.push(format!("{} {}", city, cat.to_lowercase()));
            m_lat.push(la);
            m_lon.push(lo);
            m_cat.push((*cat).to_string());
            m_risk.push(if *cat == "ELECTRONICS" || *cat == "TRAVEL" {
                "HIGH".to_string()
            } else {
                "LOW".to_string()
            });
            m_city.push((*city).to_string());
            m_pin.push((*pin).to_string());
            m_state.push((*state).to_string());
            osm += 1;
        }
    }

    let mut res_df = df!(
        "osm_id" => r_osm,
        "h3_index" => r_h3,
        "latitude" => r_lat,
        "longitude" => r_lon,
        "city" => r_city,
        "pincode" => r_pin,
        "state" => r_state
    )
    .expect("residential df");

    let mut merch_df = df!(
        "osm_id" => m_osm,
        "h3_index" => m_h3,
        "merchant_name" => m_name,
        "latitude" => m_lat,
        "longitude" => m_lon,
        "merchant_category" => m_cat,
        "risk_level" => m_risk,
        "city" => m_city,
        "pincode" => m_pin,
        "state" => m_state
    )
    .expect("merchants df");

    let mut rf = File::create(dir.join("ref_residential.parquet"))?;
    ParquetWriter::new(&mut rf)
        .finish(&mut res_df)
        .map_err(std::io::Error::other)?;

    let mut mf = File::create(dir.join("ref_merchants.parquet"))?;
    ParquetWriter::new(&mut mf)
        .finish(&mut merch_df)
        .map_err(std::io::Error::other)?;

    Ok(())
}
