//! Generic Parquet loading + validation helpers, plus the error type they return.
//!
//! These are the table-agnostic half of the reference-data contract: open a file,
//! check required columns exist, check they have the expected dtypes, and ensure
//! the table is non-empty. The domain supplies the table names and column lists.

use polars::prelude::*;
use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Errors produced while reading or validating an external table.
#[derive(Debug)]
pub enum ReferenceError {
    /// A file could not be opened (usually missing).
    OpenFailed {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A file is not readable as Parquet.
    Read { path: PathBuf, source: PolarsError },
    /// A table is missing one or more required columns.
    MissingColumns {
        table: String,
        missing: Vec<String>,
        found: Vec<String>,
    },
    /// A required column exists but has an unexpected type.
    WrongDtype {
        table: String,
        column: String,
        message: String,
    },
    /// A table contains no rows.
    Empty { table: String },
    /// A required column contains null values.
    NullValues { table: String, column: String },
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReferenceError::OpenFailed { path, source } => {
                write!(f, "cannot open reference file {}: {source}", path.display())
            }
            ReferenceError::Read { path, source } => {
                write!(
                    f,
                    "cannot read reference Parquet {}: {source}",
                    path.display()
                )
            }
            ReferenceError::MissingColumns {
                table,
                missing,
                found,
            } => write!(
                f,
                "{table} is missing required column(s) [{}]; found [{}]",
                missing.join(", "),
                found.join(", ")
            ),
            ReferenceError::WrongDtype {
                table,
                column,
                message,
            } => {
                write!(
                    f,
                    "{table}: column '{column}' has the wrong type: {message}"
                )
            }
            ReferenceError::Empty { table } => write!(
                f,
                "{table} is empty — it must contain at least one reference point"
            ),
            ReferenceError::NullValues { table, column } => {
                write!(
                    f,
                    "{table}: required column '{column}' contains null values"
                )
            }
        }
    }
}

impl std::error::Error for ReferenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReferenceError::OpenFailed { source, .. } => Some(source),
            ReferenceError::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Read a Parquet file into a `DataFrame`.
pub fn read_parquet(path: &Path) -> Result<DataFrame, ReferenceError> {
    let file = File::open(path).map_err(|source| ReferenceError::OpenFailed {
        path: path.to_path_buf(),
        source,
    })?;
    ParquetReader::new(file)
        .finish()
        .map_err(|source| ReferenceError::Read {
            path: path.to_path_buf(),
            source,
        })
}

/// Fail if any of `expected` is missing from `df`.
pub fn check_columns(df: &DataFrame, table: &str, expected: &[&str]) -> Result<(), ReferenceError> {
    let found: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let missing: Vec<String> = expected
        .iter()
        .copied()
        .filter(|name| !found.iter().any(|f| f.as_str() == *name))
        .map(String::from)
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(ReferenceError::MissingColumns {
            table: table.to_string(),
            missing,
            found,
        })
    }
}

/// Fail if `df` has no rows.
pub fn ensure_non_empty(df: &DataFrame, table: &str) -> Result<(), ReferenceError> {
    if df.height() == 0 {
        Err(ReferenceError::Empty {
            table: table.to_string(),
        })
    } else {
        Ok(())
    }
}

/// Fail if any of `columns` contains null values.
///
/// Required columns are later read with `into_no_null_iter()`, which silently skips
/// nulls and would misalign the extracted vectors. Validate up front instead.
pub fn ensure_no_nulls(
    df: &DataFrame,
    table: &str,
    columns: &[&str],
) -> Result<(), ReferenceError> {
    for name in columns {
        if require_column(df, table, name)?.null_count() > 0 {
            return Err(ReferenceError::NullValues {
                table: table.to_string(),
                column: (*name).to_string(),
            });
        }
    }
    Ok(())
}

/// Borrow a column, mapping a missing column to a descriptive error.
pub fn require_column<'a>(
    df: &'a DataFrame,
    table: &str,
    column: &str,
) -> Result<&'a Column, ReferenceError> {
    df.column(column)
        .map_err(|_| ReferenceError::MissingColumns {
            table: table.to_string(),
            missing: vec![column.to_string()],
            found: df
                .get_column_names()
                .iter()
                .map(|s| s.to_string())
                .collect(),
        })
}

/// Borrow a string column, mapping a dtype mismatch to a descriptive error.
pub fn string_column<'a>(
    df: &'a DataFrame,
    table: &str,
    column: &str,
) -> Result<&'a StringChunked, ReferenceError> {
    require_column(df, table, column)?
        .str()
        .map_err(|e| ReferenceError::WrongDtype {
            table: table.to_string(),
            column: column.to_string(),
            message: e.to_string(),
        })
}

/// Borrow an f64 column, mapping a dtype mismatch to a descriptive error.
pub fn float_column<'a>(
    df: &'a DataFrame,
    table: &str,
    column: &str,
) -> Result<&'a Float64Chunked, ReferenceError> {
    require_column(df, table, column)?
        .f64()
        .map_err(|e| ReferenceError::WrongDtype {
            table: table.to_string(),
            column: column.to_string(),
            message: e.to_string(),
        })
}

/// Borrow an i64 column, mapping a dtype mismatch to a descriptive error.
pub fn int_column<'a>(
    df: &'a DataFrame,
    table: &str,
    column: &str,
) -> Result<&'a Int64Chunked, ReferenceError> {
    require_column(df, table, column)?
        .i64()
        .map_err(|e| ReferenceError::WrongDtype {
            table: table.to_string(),
            column: column.to_string(),
            message: e.to_string(),
        })
}
