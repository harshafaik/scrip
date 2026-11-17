#![cfg(feature = "worldbuild")]

use polars::prelude::*;
use scrip::worldbuild::{write_reference, MerchantRow, ResidentialRow};
use std::fs::File;
use std::path::Path;

fn read_columns(path: &Path) -> Vec<String> {
    let df = ParquetReader::new(File::open(path).unwrap())
        .finish()
        .unwrap();
    df.get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn written_references_satisfy_the_contract() {
    let tmp = tempfile::tempdir().unwrap();

    let residential = vec![
        ResidentialRow {
            osm_id: 11,
            h3_index: "88283082d1fffff".to_string(),
            latitude: 19.076,
            longitude: 72.877,
            city: Some("MUMBAI".to_string()),
            pincode: Some("400001".to_string()),
            state: "Maharashtra".to_string(),
            district_name: Some("Mumbai".to_string()),
        },
        ResidentialRow {
            osm_id: 12,
            h3_index: "88283082d1fffff".to_string(),
            latitude: 19.077,
            longitude: 72.878,
            city: None,
            pincode: None,
            state: "Maharashtra".to_string(),
            district_name: None,
        },
    ];

    let merchants = vec![MerchantRow {
        osm_id: 21,
        h3_index: "88283082d1fffff".to_string(),
        merchant_name: "Test Grocer".to_string(),
        latitude: 19.076,
        longitude: 72.877,
        merchant_category: "GROCERY".to_string(),
        risk_level: "LOW".to_string(),
        city: Some("MUMBAI".to_string()),
        pincode: Some("400001".to_string()),
        state: "Maharashtra".to_string(),
        district_name: Some("Mumbai".to_string()),
    }];

    write_reference(&residential, &merchants, tmp.path()).unwrap();

    let report = scrip::reference::validate(tmp.path()).unwrap();
    assert_eq!(report.residential_rows, 2);
    assert_eq!(report.merchant_rows, 1);

    assert_eq!(
        read_columns(&tmp.path().join("ref_residential.parquet")),
        vec![
            "osm_id",
            "h3_index",
            "latitude",
            "longitude",
            "city",
            "pincode",
            "state",
            "district_name"
        ]
    );
    assert_eq!(
        read_columns(&tmp.path().join("ref_merchants.parquet")),
        vec![
            "osm_id",
            "h3_index",
            "latitude",
            "longitude",
            "merchant_name",
            "merchant_category",
            "risk_level",
            "city",
            "pincode",
            "state",
            "district_name"
        ]
    );
}
