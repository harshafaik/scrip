use scrip::reference::{self, REFERENCE_SCHEMA_VERSION};

#[test]
fn valid_fixtures_pass_validation() {
    let tmp = tempfile::tempdir().unwrap();
    scrip::fixtures::write_reference_fixtures(tmp.path()).unwrap();

    let report = reference::validate(tmp.path()).unwrap();
    assert_eq!(report.schema_version, REFERENCE_SCHEMA_VERSION);
    assert!(report.residential_rows > 0);
    assert!(report.merchant_rows > 0);
}

#[test]
fn missing_file_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    let err = reference::validate(tmp.path()).unwrap_err();
    assert!(
        err.to_string().contains("cannot open reference file"),
        "unexpected error: {err}"
    );
}

#[test]
fn corrupt_file_is_reported() {
    let tmp = tempfile::tempdir().unwrap();
    scrip::fixtures::write_reference_fixtures(tmp.path()).unwrap();
    std::fs::write(tmp.path().join(reference::MERCHANTS_FILE), b"not parquet").unwrap();

    let err = reference::validate(tmp.path()).unwrap_err();
    assert!(
        err.to_string().contains("cannot read reference Parquet"),
        "unexpected error: {err}"
    );
}
