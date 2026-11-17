//! OSM → H3 → Parquet reference builder.
//!
//! Reads an OpenStreetMap `.osm.pbf`, classifies residential and merchant nodes,
//! enriches them with state/category seed maps and authoritative boundary
//! polygons, and writes `ref_residential.parquet` / `ref_merchants.parquet` in the
//! schema [`crate::reference`] expects.
//!
//! Enabled with the `worldbuild` feature:
//!
//! ```sh
//! cargo run --features worldbuild --bin scrip-world -- build \
//!   --pbf india.osm.pbf --boundaries boundaries/ --out-dir references
//! ```

mod boundaries;
mod extract;
mod normalize;
mod seeds;
mod write;

pub use write::{write as write_reference, MerchantRow, ResidentialRow};

use boundaries::Boundaries;
use extract::RawPoint;
use seeds::Seeds;
use std::error::Error;
use std::path::PathBuf;

/// Inputs for a reference build.
pub struct Options {
    pub pbf: PathBuf,
    pub boundaries_dir: PathBuf,
    pub out_dir: PathBuf,
    pub resolution: u8,
    pub state_map: Option<PathBuf>,
    pub category_map: Option<PathBuf>,
    pub state_prop: String,
    pub district_prop: String,
}

/// Run the full OSM → Parquet reference build.
pub fn run(opts: &Options) -> Result<(), Box<dyn Error>> {
    let resolution = h3o::Resolution::try_from(opts.resolution).map_err(|_| {
        format!(
            "invalid H3 resolution {} (expected 0..=15)",
            opts.resolution
        )
    })?;

    println!("Loading seed maps...");
    let seeds = Seeds::load(opts.state_map.as_deref(), opts.category_map.as_deref())?;

    println!(
        "Loading boundaries from {} ...",
        opts.boundaries_dir.display()
    );
    let boundaries = Boundaries::load(&opts.boundaries_dir, &opts.state_prop, &opts.district_prop)?;
    println!(
        "  {} state polygons, {} district polygons",
        boundaries.state_count(),
        boundaries.district_count()
    );

    println!("Reading {} ...", opts.pbf.display());
    let extracted = extract::extract(&opts.pbf, resolution)?;
    println!(
        "  {} residential nodes, {} merchant nodes",
        extracted.residential.len(),
        extracted.merchants.len()
    );

    println!("Enriching ...");
    let mut residential: Vec<ResidentialRow> = extracted
        .residential
        .iter()
        .map(|p| {
            let (state, district_name) = resolve_admin(p, &seeds, &boundaries);
            ResidentialRow {
                osm_id: p.osm_id,
                h3_index: p.h3_index.clone(),
                latitude: p.lat,
                longitude: p.lon,
                city: p.city.as_deref().and_then(normalize::city),
                pincode: p.postcode.as_deref().and_then(normalize::pincode),
                state,
                district_name,
            }
        })
        .collect();

    let mut merchants: Vec<MerchantRow> = extracted
        .merchants
        .iter()
        .map(|p| {
            let (category, risk) = seeds.category(p.sub_category.as_deref().unwrap_or(""));
            let (state, district_name) = resolve_admin(p, &seeds, &boundaries);
            MerchantRow {
                osm_id: p.osm_id,
                h3_index: p.h3_index.clone(),
                merchant_name: p
                    .name
                    .clone()
                    .unwrap_or_else(|| "Unknown Merchant".to_string()),
                latitude: p.lat,
                longitude: p.lon,
                merchant_category: category.to_string(),
                risk_level: risk.to_string(),
                city: p.city.as_deref().and_then(normalize::city),
                pincode: p.postcode.as_deref().and_then(normalize::pincode),
                state,
                district_name,
            }
        })
        .collect();

    // Parallel extraction yields nondeterministic order; sort so refs are
    // reproducible across runs.
    residential.sort_by_key(|row| row.osm_id);
    merchants.sort_by_key(|row| row.osm_id);

    println!("Writing Parquet to {} ...", opts.out_dir.display());
    write_reference(&residential, &merchants, &opts.out_dir)?;

    match crate::reference::validate(&opts.out_dir) {
        Ok(report) => println!(
            "Reference validation OK — {} residential, {} merchant rows",
            report.residential_rows, report.merchant_rows
        ),
        Err(err) => eprintln!("warning: reference validation failed: {err}"),
    }
    Ok(())
}

/// Assign an authoritative state from boundary polygons, falling back to a
/// normalized `addr:state` tag and finally `"Unknown"`, plus the boundary
/// district when the point falls inside one.
fn resolve_admin(
    point: &RawPoint,
    seeds: &Seeds,
    boundaries: &Boundaries,
) -> (String, Option<String>) {
    let state = boundaries
        .state_at(point.lon, point.lat)
        .map(str::to_string)
        .or_else(|| {
            point
                .state
                .as_deref()
                .and_then(|raw| seeds.normalize_state(raw))
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let district = boundaries
        .district_at(point.lon, point.lat)
        .map(|(_, district)| district.to_string());

    (state, district)
}
