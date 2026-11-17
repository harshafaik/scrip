# scrip

[![CI](https://github.com/harshafaik/scrip/actions/workflows/ci.yml/badge.svg)](https://github.com/harshafaik/scrip/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)

Seeded, reproducible synthetic financial data generator.

`scrip` generates referentially-consistent `Customer → Account → Card → Transaction`
lifecycles for India, with realistic geography and configurable fraud injection —
plus **ground-truth fraud labels** alongside the noisy operational `is_fraud` label.

The whole run is a pure function of `(config, params)`: no wall-clock reads, every
ID derived from a seeded RNG. The same inputs produce byte-identical output.

## System

`scrip` is the upstream generator in a three-repository system:

| Repository | Responsibility |
| :--- | :--- |
| **scrip** (this repository) | OpenStreetMap world building and deterministic agent simulation |
| [`riskfabric-pipelines`](https://github.com/harshafaik/riskfabric-pipelines) | Contract-validated ingestion, star schema, orchestration |
| [`riskfabric-ml`](https://github.com/harshafaik/riskfabric-ml) | Feature engineering, training, calibration, scoring, monitoring, case review |

The repositories are linked by the dataset contract, not shared code.

## Why

Synthetic financial data is the safe way to build and test fraud/AML/analytics
pipelines and ML models — but most generators produce either randoms with no
structure or a fixed CSV. `scrip` gives you a *reproducible* population with real
relationships, realistic spend geography, and labelled fraud you can evaluate against.

## Build

```sh
cargo build --release
```

## Quick start

```sh
# 1. Write a tiny reference dataset (cities + merchants) so generate can run.
cargo run --release -- fixtures --out-dir references

# 2. Generate a dataset.
cargo run --release -- generate \
  --customers 5000 --seed 42 --days 365 \
  --base-date 2026-01-01T00:00:00Z \
  --input-dir references --out-dir out
```

Output (`out/`): `customers.parquet`, `accounts.parquet`, `cards.parquet`,
`transactions.parquet`, `fraud_metadata.parquet`.

## Output contract (schema v2)

By default `generate` writes the five flat Parquet files. `--partition-by-day`
writes `<table>/event_date=YYYY-MM-DD/<table>.parquet` instead:
`transactions` and `fraud_metadata` use the transaction date, while
`customers`, `accounts`, and `cards` use the run's as-of date. No `event_date`
column is added.

Every file carries Parquet key/value metadata: `scrip.schema_version`,
`scrip.dataset_version`, `scrip.seed`, and `scrip.base_date`. Each run also
writes `_dataset.json` with sorted keys, no wall-clock timestamp, the resolved
parameters, and per-table row counts and SHA-256 hashes.

Schema v2 uses `Datetime(us, UTC)` for `transactions.timestamp` and `Date` for
`customers.registration_date`, `accounts.creation_date`, and
`cards.issue_date` / `activation_date` / `expiry_date`.

`--base-date` is printed on every run; omitting it warns and uses the current
time, so pass it for reproducible output. `scrip audit --input-dir out` prints
the statistical, behavioral, and leakage metrics for a flat output directory.

## Reproducibility

- Provider: `--seed` (or the config's `global.seed`).
- Window: `--days`, and `--base-date` pins the end of the window. **Set it** —
  otherwise the default is "now", and output changes day to day.
- All entity ids are derived from the seeded RNG, so nothing is random-but-hidden.

Two runs with identical arguments produce identical files. There is a test for it
(`tests/determinism.rs`).

## Configuration

Configuration is layered: built-in defaults (the YAML embedded from `config/`),
then `--config-dir <dir>`, then CLI flags — flags win. A config directory contains:

```
fraud_rules.yaml  fraud_tuning.yaml  customer_config.yaml
transaction_config.yaml
```

Dataset shape is a first-class CLI surface; each flag overrides the matching
config field:

| Flag | Overrides |
| --- | --- |
| `--customers N` | `customer.control.customer_count` |
| `--accounts-per-customer N` or `MIN..MAX` | `customer.control.accounts_per_customer` |
| `--cards-per-account N` or `MIN..MAX` | `customer.control.cards_per_account` |
| `--txns-per-customer N` or `MIN..MAX` | `customer.control.transactions_per_customer` |
| `--days N` | `transactions.lookback_days` |

`scrip config [flags]` prints the effective configuration as YAML after applying
the overrides — useful for checking a shape before a long run:

```sh
scrip config --customers 5000 --accounts-per-customer 1..3 --cards-per-account 2
```

## Reference data

`generate` consumes two Parquet files in `--input-dir`:

| File | Columns |
| --- | --- |
| `ref_residential.parquet` | `h3_index, latitude, longitude, city, pincode, state` |
| `ref_merchants.parquet` | `osm_id, h3_index, merchant_name, latitude, longitude, merchant_category, state` |

`scrip fixtures` writes minimal versions of both. In production these come from a
separate geospatial pipeline (OSM → H3 → Parquet).

## Library use

```rust
use scrip::{generate_all, AppConfig, GenParams};

let cfg = AppConfig::default_embedded();
let params = GenParams::new(42, 1000, 365).input_dir("references").output_dir("out");
let data = generate_all(&cfg, &params)?; // Result<Dataset, ReferenceError>
```

## License

MIT
