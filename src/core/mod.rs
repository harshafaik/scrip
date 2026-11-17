//! Domain-agnostic engine primitives.
//!
//! Everything here is independent of the financial data model. The dependency rule
//! is one-directional: modules in `core` must **never** reference the domain modules
//! (`config`, `models`, `generators`, `fixtures`, `reference`). The domain may depend
//! on `core`. `tests/architecture.rs` enforces this.
//!
//! This is intentionally a plain set of functions and types (no trait framework),
//! laid out so it can be promoted to a standalone `scrip-core` crate without any
//! internal changes: the module boundary is the crate boundary.

pub mod arrival;
pub mod distribution;
pub mod labels;
pub mod parquet;
pub mod rng;
pub mod sample;
pub mod time;
pub mod writer;
