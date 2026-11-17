use chrono::{DateTime, Utc};
use clap::{Args, Parser, Subcommand};
use scrip::config::{AppConfig, RangeControl};
use scrip::generators::run::{generate_all, GenParams};
use scrip::output::{write_dataset, write_dataset_partitioned, DatasetMeta};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "scrip",
    about = "Seeded, reproducible synthetic financial data generator",
    version
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write a tiny reference dataset (for demos/tests) so `generate` can run.
    Fixtures {
        #[arg(long, default_value = "references")]
        out_dir: PathBuf,
    },
    /// Validate a reference directory against the expected schema (alias: doctor).
    #[command(visible_alias = "doctor")]
    ValidateRefs {
        /// Directory holding ref_residential.parquet / ref_merchants.parquet.
        #[arg(long, default_value = "references")]
        input_dir: PathBuf,
    },
    /// Print the effective configuration as YAML, after applying any overrides.
    Config {
        #[command(flatten)]
        cfg: ConfigArgs,
    },
    /// Print statistical, behavioral, and leakage metrics for a generated dataset.
    Audit {
        /// Directory holding the generated Parquet tables.
        #[arg(long, default_value = "out")]
        input_dir: PathBuf,
    },
    /// Generate a dataset from a reference directory.
    Generate {
        #[command(flatten)]
        cfg: ConfigArgs,
        /// RNG seed (defaults to the config's seed).
        #[arg(long)]
        seed: Option<u64>,
        /// Fixed end-of-window timestamp (RFC3339). Defaults to now — set it for
        /// fully reproducible output.
        #[arg(long)]
        base_date: Option<String>,
        /// Directory holding ref_residential.parquet / ref_merchants.parquet.
        #[arg(long)]
        input_dir: Option<PathBuf>,
        /// Write event_date=YYYY-MM-DD partitions instead of the flat layout.
        #[arg(long)]
        partition_by_day: bool,
        /// Output directory for the generated Parquet tables.
        #[arg(long, default_value = "out")]
        out_dir: PathBuf,
    },
}

/// An inclusive count spec accepted on the CLI: `N` (fixed) or `MIN..MAX`.
#[derive(Debug, Clone, Copy)]
struct CountRange {
    min: usize,
    max: usize,
}

fn parse_count_range(s: &str) -> Result<CountRange, String> {
    let s = s.trim();
    if let Some((lo, hi)) = s.split_once("..") {
        let min = lo
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("invalid range start in '{s}'"))?;
        let max = hi
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("invalid range end in '{s}'"))?;
        if min > max {
            return Err(format!("invalid range '{s}': min is greater than max"));
        }
        Ok(CountRange { min, max })
    } else {
        let n = s
            .parse::<usize>()
            .map_err(|_| format!("expected `N` or `MIN..MAX`, got '{s}'"))?;
        Ok(CountRange { min: n, max: n })
    }
}

/// Configuration options shared by `generate` and `config`. Every field is an
/// override: when omitted, the value comes from the config file (or the
/// embedded defaults).
#[derive(Args)]
struct ConfigArgs {
    /// Config directory; defaults to the built-in embedded config.
    #[arg(long)]
    config_dir: Option<PathBuf>,
    /// Number of customers (overrides `control.customer_count`).
    #[arg(long)]
    customers: Option<usize>,
    /// Transaction window length in days (overrides `transactions.lookback_days`).
    #[arg(long)]
    days: Option<i64>,
    /// Accounts per customer: `N` or `MIN..MAX`.
    #[arg(long, value_parser = parse_count_range)]
    accounts_per_customer: Option<CountRange>,
    /// Cards per account: `N` or `MIN..MAX`.
    #[arg(long, value_parser = parse_count_range)]
    cards_per_account: Option<CountRange>,
    /// Transactions per customer: `N` or `MIN..MAX`.
    #[arg(long, value_parser = parse_count_range)]
    txns_per_customer: Option<CountRange>,
}

impl ConfigArgs {
    /// Resolve the effective config: embedded defaults, then `--config-dir` if
    /// given, then CLI overrides (flags win).
    fn resolve(&self) -> AppConfig {
        let mut config = match &self.config_dir {
            Some(dir) => AppConfig::from_dir(dir),
            None => AppConfig::default_embedded(),
        };

        if let Some(n) = self.customers {
            config.customer.control.customer_count = n;
        }
        if let Some(d) = self.days {
            config.transactions.transactions.lookback_days = d as i32;
        }
        if let Some(r) = self.accounts_per_customer {
            config.customer.control.accounts_per_customer = RangeControl {
                min: r.min,
                max: r.max,
            };
        }
        if let Some(r) = self.cards_per_account {
            config.customer.control.cards_per_account = RangeControl {
                min: r.min,
                max: r.max,
            };
        }
        if let Some(r) = self.txns_per_customer {
            config.customer.control.transactions_per_customer = RangeControl {
                min: r.min,
                max: r.max,
            };
        }

        config
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().cmd {
        Command::Fixtures { out_dir } => {
            scrip::fixtures::write_reference_fixtures(&out_dir)?;
            println!("wrote reference fixtures to {}", out_dir.display());
        }
        Command::ValidateRefs { input_dir } => match scrip::reference::validate(&input_dir) {
            Ok(report) => {
                println!(
                    "reference schema v{} OK — {}",
                    report.schema_version,
                    input_dir.display()
                );
                println!(
                    "  ref_residential.parquet: {} rows [{}]",
                    report.residential_rows,
                    report.residential_columns.join(", ")
                );
                println!(
                    "  ref_merchants.parquet:   {} rows [{}]",
                    report.merchant_rows,
                    report.merchant_columns.join(", ")
                );
            }
            Err(e) => {
                eprintln!(
                    "reference validation failed for {}:\n  {e}",
                    input_dir.display()
                );
                std::process::exit(1);
            }
        },
        Command::Audit { input_dir } => {
            let report = scrip::metrics::audit_dir(&input_dir)?;
            scrip::metrics::print_report(&report);
        }
        Command::Config { cfg } => {
            print!("{}", serde_yaml::to_string(&cfg.resolve())?);
        }
        Command::Generate {
            cfg,
            seed,
            base_date,
            input_dir,
            partition_by_day,
            out_dir,
        } => {
            let config = cfg.resolve();
            let seed = seed.unwrap_or(config.rules.global.seed as u64);
            let base_date_was_defaulted = base_date.is_none();
            let base_end_date = match base_date {
                Some(s) => DateTime::parse_from_rfc3339(&s)?.with_timezone(&Utc),
                None => Utc::now(),
            };

            println!("base_date={}", base_end_date.to_rfc3339());
            if base_date_was_defaulted {
                eprintln!(
                    "warning: --base-date not set; using {}. \
                     Output is not reproducible without a fixed --base-date.",
                    base_end_date.to_rfc3339()
                );
            }

            let params = GenParams {
                seed,
                customers: config.customer.control.customer_count,
                days: config.transactions.transactions.lookback_days as i64,
                base_end_date,
                input_dir: input_dir.unwrap_or_else(|| PathBuf::from("references")),
                output_dir: out_dir,
            };

            let meta = DatasetMeta {
                seed,
                base_date: base_end_date,
                days: config.transactions.transactions.lookback_days as i64,
                customers: config.customer.control.customer_count,
                accounts_per_customer: config.customer.control.accounts_per_customer,
                cards_per_account: config.customer.control.cards_per_account,
                transactions_per_customer: config.customer.control.transactions_per_customer,
            };

            let started = std::time::Instant::now();
            let data = generate_all(&config, &params)?;
            if partition_by_day {
                write_dataset_partitioned(&data, &params.output_dir, &meta)?;
            } else {
                write_dataset(&data, &params.output_dir, &meta)?;
            }

            println!(
                "seed={} | customers={} accounts={} cards={} transactions={} fraud_labels={} | {:.2}s",
                seed,
                data.customers.len(),
                data.accounts.len(),
                data.cards.len(),
                data.transactions.len(),
                data.fraud_metadata.len(),
                started.elapsed().as_secs_f64()
            );
            println!("wrote Parquet to {}", params.output_dir.display());
        }
    }
    Ok(())
}
