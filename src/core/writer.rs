use polars::io::parquet::write::KeyValueMetadata;
use polars::prelude::*;
use std::fs::File;
use std::path::Path;

/// Write a `DataFrame` to a Parquet file at `path`.
pub fn write_parquet(df: &mut DataFrame, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    ParquetWriter::new(file).finish(df)?;
    Ok(())
}

/// Write a `DataFrame` with file-level key/value metadata.
pub fn write_parquet_with_metadata(
    df: &mut DataFrame,
    path: &Path,
    metadata: Vec<(String, String)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(path)?;
    ParquetWriter::new(file)
        .with_key_value_metadata(Some(KeyValueMetadata::from_static(metadata)))
        .finish(df)?;
    Ok(())
}
