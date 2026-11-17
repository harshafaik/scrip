//! **scrip** — seeded, reproducible synthetic financial transaction data.
//!
//! Generates referentially-consistent `Customer → Account → Card → Transaction`
//! lifecycles with realistic geography and configurable fraud injection, plus
//! ground-truth labels alongside the noisy operational `is_fraud` label.
//!
//! The run is a pure function of `(config, params)`: no wall-clock reads, all
//! IDs derived from seeded RNG — identical inputs produce byte-identical output.
//!
//! ```
//! use scrip::generators::run::{generate_all, GenParams};
//! use scrip::config::AppConfig;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dir = std::env::temp_dir().join("scrip_doctest");
//! scrip::fixtures::write_reference_fixtures(&dir)?;
//! let cfg = AppConfig::default_embedded();
//! let params = GenParams::new(42, 10, 30).input_dir(&dir).output_dir(&dir);
//! let data = generate_all(&cfg, &params)?;
//! assert_eq!(data.customers.len(), 10);
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod core;
pub mod fixtures;
pub mod generators;
pub mod metrics;
pub mod models;
pub mod output;
pub mod reference;
pub mod schema;
#[cfg(feature = "worldbuild")]
pub mod worldbuild;

pub use config::AppConfig;
pub use generators::run::{generate_all, Dataset, GenParams};
pub use reference::{validate as validate_references, ReferenceError, ReferenceReport};
