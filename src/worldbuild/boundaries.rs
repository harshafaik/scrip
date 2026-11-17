//! Boundary enrichment: load state/district polygons from GeoJSON and assign a
//! point to a region by bounding-box prefilter + point-in-polygon.

use geo::{BoundingRect, Contains, MultiPolygon, Point, Polygon};
use geojson::{FeatureCollection, GeometryValue};
use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

/// Coarse grid cell size for the polygon index, in degrees.
const GRID_CELL_DEGREES: f64 = 0.5;

pub struct Region {
    state: String,
    district: Option<String>,
    geom: MultiPolygon<f64>,
    /// (min_lon, min_lat, max_lon, max_lat)
    bbox: (f64, f64, f64, f64),
}

pub struct Boundaries {
    states: Vec<Region>,
    districts: Vec<Region>,
    state_index: GridIndex,
    district_index: GridIndex,
}

impl Boundaries {
    pub fn load(dir: &Path, state_prop: &str, district_prop: &str) -> Result<Self, Box<dyn Error>> {
        let states = load_regions(&dir.join("states.geojson"), state_prop, None)?;
        let districts = load_regions(
            &dir.join("districts.geojson"),
            state_prop,
            Some(district_prop),
        )?;
        let state_index = GridIndex::build(&states);
        let district_index = GridIndex::build(&districts);
        Ok(Self {
            states,
            districts,
            state_index,
            district_index,
        })
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    pub fn district_count(&self) -> usize {
        self.districts.len()
    }

    /// Authoritative state containing `(lon, lat)`.
    pub fn state_at(&self, lon: f64, lat: f64) -> Option<&str> {
        find_region(&self.states, &self.state_index, lon, lat).map(|region| region.state.as_str())
    }

    /// Authoritative `(state, district)` containing `(lon, lat)`.
    pub fn district_at(&self, lon: f64, lat: f64) -> Option<(&str, &str)> {
        find_region(&self.districts, &self.district_index, lon, lat).and_then(|region| {
            region
                .district
                .as_deref()
                .map(|d| (region.state.as_str(), d))
        })
    }
}

/// A coarse lat/lon grid mapping each cell to the regions whose bounding box
/// overlaps it. A point can only fall inside a region whose bbox overlaps the
/// point's cell, so this replaces a linear scan over every polygon.
struct GridIndex {
    buckets: HashMap<(i32, i32), Vec<usize>>,
}

impl GridIndex {
    fn build(regions: &[Region]) -> Self {
        let mut buckets: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (idx, region) in regions.iter().enumerate() {
            let (min_lon, min_lat, max_lon, max_lat) = region.bbox;
            let (x0, y0) = grid_cell(min_lon, min_lat);
            let (x1, y1) = grid_cell(max_lon, max_lat);
            for x in x0..=x1 {
                for y in y0..=y1 {
                    buckets.entry((x, y)).or_default().push(idx);
                }
            }
        }
        Self { buckets }
    }

    fn candidates(&self, lon: f64, lat: f64) -> &[usize] {
        self.buckets
            .get(&grid_cell(lon, lat))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

fn grid_cell(lon: f64, lat: f64) -> (i32, i32) {
    (
        (lon / GRID_CELL_DEGREES).floor() as i32,
        (lat / GRID_CELL_DEGREES).floor() as i32,
    )
}

fn find_region<'a>(
    regions: &'a [Region],
    index: &GridIndex,
    lon: f64,
    lat: f64,
) -> Option<&'a Region> {
    let point = Point::new(lon, lat);
    index
        .candidates(lon, lat)
        .iter()
        .filter_map(|&i| regions.get(i))
        .find(|region| {
            let (min_lon, min_lat, max_lon, max_lat) = region.bbox;
            lon >= min_lon
                && lon <= max_lon
                && lat >= min_lat
                && lat <= max_lat
                && region.geom.contains(&point)
        })
}

fn load_regions(
    path: &Path,
    state_prop: &str,
    district_prop: Option<&str>,
) -> Result<Vec<Region>, Box<dyn Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let collection: FeatureCollection = text
        .parse()
        .map_err(|e| format!("cannot parse {}: {e}", path.display()))?;

    let mut regions = Vec::new();
    for feature in collection.features {
        let Some(geometry) = feature.geometry.as_ref() else {
            continue;
        };
        let geom: MultiPolygon<f64> = match &geometry.value {
            GeometryValue::Polygon { .. } => {
                let polygon: Polygon<f64> = (&geometry.value)
                    .try_into()
                    .map_err(|e| format!("{}: invalid polygon: {e}", path.display()))?;
                MultiPolygon::new(vec![polygon])
            }
            GeometryValue::MultiPolygon { .. } => (&geometry.value)
                .try_into()
                .map_err(|e| format!("{}: invalid multipolygon: {e}", path.display()))?,
            _ => continue,
        };

        let Some(properties) = feature.properties.as_ref() else {
            continue;
        };
        let get = |key: &str| {
            properties
                .get(key)
                .and_then(|value| value.as_str())
                .map(str::to_string)
        };

        let Some(state) = get(state_prop) else {
            continue;
        };
        let district = district_prop.and_then(get);

        let rect = geom
            .bounding_rect()
            .ok_or_else(|| format!("{}: empty boundary geometry", path.display()))?;
        let bbox = (rect.min().x, rect.min().y, rect.max().x, rect.max().y);

        regions.push(Region {
            state,
            district,
            geom,
            bbox,
        });
    }

    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixtures(dir: &Path) {
        let states = r#"{
          "type": "FeatureCollection",
          "features": [{
            "type": "Feature",
            "properties": {"st_nm": "TestState"},
            "geometry": {"type": "Polygon", "coordinates": [[[0,0],[10,0],[10,10],[0,10],[0,0]]]}
          }]
        }"#;
        let districts = r#"{
          "type": "FeatureCollection",
          "features": [{
            "type": "Feature",
            "properties": {"st_nm": "TestState", "district": "TestDistrict"},
            "geometry": {"type": "Polygon", "coordinates": [[[0,0],[10,0],[10,10],[0,10],[0,0]]]}
          }]
        }"#;
        std::fs::write(dir.join("states.geojson"), states).unwrap();
        std::fs::write(dir.join("districts.geojson"), districts).unwrap();
    }

    #[test]
    fn assigns_points_inside_and_outside() {
        let tmp = tempfile::tempdir().unwrap();
        write_fixtures(tmp.path());

        let boundaries = Boundaries::load(tmp.path(), "st_nm", "district").unwrap();
        assert_eq!(boundaries.state_count(), 1);
        assert_eq!(boundaries.district_count(), 1);
        assert_eq!(boundaries.state_at(5.0, 5.0), Some("TestState"));
        assert_eq!(
            boundaries.district_at(5.0, 5.0),
            Some(("TestState", "TestDistrict"))
        );
        assert_eq!(boundaries.state_at(20.0, 20.0), None);
    }
}
