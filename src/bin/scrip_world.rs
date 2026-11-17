use clap::{Parser, Subcommand};
use scrip::worldbuild::{self, Options};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "scrip-world",
    about = "Build scrip reference Parquet files from OpenStreetMap data"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Extract OSM nodes, enrich with boundaries, and write reference Parquets.
    Build {
        /// Path to an OpenStreetMap `.osm.pbf` extract.
        #[arg(long)]
        pbf: PathBuf,
        /// Directory containing `states.geojson` and `districts.geojson`.
        #[arg(long)]
        boundaries: PathBuf,
        /// Output directory for `ref_residential.parquet` / `ref_merchants.parquet`.
        #[arg(long, default_value = "references")]
        out_dir: PathBuf,
        /// H3 resolution for the `h3_index` column.
        #[arg(long, default_value_t = 8)]
        resolution: u8,
        /// Optional state-map CSV (defaults to the embedded India map).
        #[arg(long)]
        state_map: Option<PathBuf>,
        /// Optional category-map CSV (defaults to the embedded map).
        #[arg(long)]
        category_map: Option<PathBuf>,
        /// GeoJSON property holding the state name.
        #[arg(long, default_value = "st_nm")]
        state_prop: String,
        /// GeoJSON property holding the district name.
        #[arg(long, default_value = "district")]
        district_prop: String,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Build {
            pbf,
            boundaries,
            out_dir,
            resolution,
            state_map,
            category_map,
            state_prop,
            district_prop,
        } => worldbuild::run(&Options {
            pbf,
            boundaries_dir: boundaries,
            out_dir,
            resolution,
            state_map,
            category_map,
            state_prop,
            district_prop,
        }),
    }
}
