//! OSM PBF extraction: read nodes in parallel and classify them as residential or
//! merchant points.

use h3o::{LatLng, Resolution};
use osmpbf::{Element, ElementReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RawPoint {
    pub osm_id: i64,
    pub h3_index: String,
    pub lat: f64,
    pub lon: f64,
    pub city: Option<String>,
    pub postcode: Option<String>,
    pub state: Option<String>,
    pub name: Option<String>,
    pub sub_category: Option<String>,
}

pub struct Extracted {
    pub residential: Vec<RawPoint>,
    pub merchants: Vec<RawPoint>,
}

pub fn extract(pbf: &Path, resolution: Resolution) -> Result<Extracted, osmpbf::Error> {
    let reader = ElementReader::from_path(pbf)?;

    let (residential, merchants) = reader.par_map_reduce(
        |element| {
            let mut residential = Vec::new();
            let mut merchants = Vec::new();

            match element {
                Element::Node(node) => {
                    push_classified(
                        node.id(),
                        node.lat(),
                        node.lon(),
                        node.tags(),
                        resolution,
                        &mut residential,
                        &mut merchants,
                    );
                }
                Element::DenseNode(node) => {
                    push_classified(
                        node.id(),
                        node.lat(),
                        node.lon(),
                        node.tags(),
                        resolution,
                        &mut residential,
                        &mut merchants,
                    );
                }
                _ => {}
            }

            (residential, merchants)
        },
        || (Vec::new(), Vec::new()),
        |mut acc, next| {
            acc.0.extend(next.0);
            acc.1.extend(next.1);
            acc
        },
    )?;

    Ok(Extracted {
        residential,
        merchants,
    })
}

fn push_classified<'a>(
    id: i64,
    lat: f64,
    lon: f64,
    tags: impl Iterator<Item = (&'a str, &'a str)>,
    resolution: Resolution,
    residential: &mut Vec<RawPoint>,
    merchants: &mut Vec<RawPoint>,
) {
    let tags = NodeTags::from_tags(tags);
    let (res, merch) = classify(id, lat, lon, &tags, resolution);
    if let Some(point) = res {
        residential.push(point);
    }
    if let Some(point) = merch {
        merchants.push(point);
    }
}

/// The handful of OSM tags the classifier needs, collected in one pass so no
/// per-node `HashMap` is allocated.
#[derive(Debug, Default)]
pub(crate) struct NodeTags<'a> {
    city: Option<&'a str>,
    postcode: Option<&'a str>,
    state: Option<&'a str>,
    name: Option<&'a str>,
    shop: Option<&'a str>,
    amenity: Option<&'a str>,
    tourism: Option<&'a str>,
    building: Option<&'a str>,
    landuse: Option<&'a str>,
    has_housenumber: bool,
    has_street: bool,
}

impl<'a> NodeTags<'a> {
    fn from_tags(tags: impl Iterator<Item = (&'a str, &'a str)>) -> Self {
        let mut parsed = Self::default();
        for (key, value) in tags {
            match key {
                "addr:city" => parsed.city = Some(value),
                "addr:postcode" => parsed.postcode = Some(value),
                "addr:state" => parsed.state = Some(value),
                "name" => parsed.name = Some(value),
                "shop" => parsed.shop = Some(value),
                "amenity" => parsed.amenity = Some(value),
                "tourism" => parsed.tourism = Some(value),
                "building" => parsed.building = Some(value),
                "landuse" => parsed.landuse = Some(value),
                "addr:housenumber" => parsed.has_housenumber = true,
                "addr:street" => parsed.has_street = true,
                _ => {}
            }
        }
        parsed
    }
}

/// Classify a single tagged node. Returns `(residential, merchant)`.
pub(crate) fn classify(
    id: i64,
    lat: f64,
    lon: f64,
    tags: &NodeTags<'_>,
    resolution: Resolution,
) -> (Option<RawPoint>, Option<RawPoint>) {
    let Ok(coord) = LatLng::new(lat, lon) else {
        return (None, None);
    };
    let h3_index = coord.to_cell(resolution).to_string();

    let city = tags.city.map(str::to_string);
    let postcode = tags.postcode.map(str::to_string);
    let state = tags.state.map(str::to_string);

    let sub_category = merchant_sub_category(tags);
    let merchant = sub_category.as_ref().map(|sub| RawPoint {
        osm_id: id,
        h3_index: h3_index.clone(),
        lat,
        lon,
        city: city.clone(),
        postcode: postcode.clone(),
        state: state.clone(),
        name: tags.name.map(str::to_string),
        sub_category: Some(sub.clone()),
    });

    let has_addr = tags.has_housenumber || tags.has_street;
    let is_residential =
        tags.building == Some("residential") || tags.landuse == Some("residential");

    let residential = if merchant.is_none() && (is_residential || has_addr) {
        Some(RawPoint {
            osm_id: id,
            h3_index,
            lat,
            lon,
            city,
            postcode,
            state,
            name: None,
            sub_category: None,
        })
    } else {
        None
    };

    (residential, merchant)
}

fn merchant_sub_category(tags: &NodeTags<'_>) -> Option<String> {
    if let Some(shop) = tags.shop {
        return Some(shop.to_string());
    }
    if let Some(amenity) = tags.amenity {
        if matches!(
            amenity,
            "restaurant" | "cafe" | "fast_food" | "bar" | "pub" | "fuel" | "cinema" | "pharmacy"
        ) {
            return Some(amenity.to_string());
        }
    }
    if let Some(tourism) = tags.tourism {
        if matches!(tourism, "hotel" | "motel" | "guest_house") {
            return Some(tourism.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags<'a>(pairs: &[(&'a str, &'a str)]) -> NodeTags<'a> {
        NodeTags::from_tags(pairs.iter().copied())
    }

    #[test]
    fn classifies_merchants() {
        let (res, merch) = classify(
            1,
            19.0,
            72.0,
            &tags(&[("shop", "convenience")]),
            Resolution::Eight,
        );
        assert!(res.is_none());
        let merch = merch.expect("merchant");
        assert_eq!(merch.sub_category.as_deref(), Some("convenience"));

        let (_, merch) = classify(
            2,
            19.0,
            72.0,
            &tags(&[("amenity", "cafe")]),
            Resolution::Eight,
        );
        assert_eq!(merch.unwrap().sub_category.as_deref(), Some("cafe"));

        let (_, merch) = classify(
            3,
            19.0,
            72.0,
            &tags(&[("amenity", "bank")]),
            Resolution::Eight,
        );
        assert!(merch.is_none());
    }

    #[test]
    fn classifies_residential() {
        let (res, merch) = classify(
            4,
            19.0,
            72.0,
            &tags(&[("building", "residential")]),
            Resolution::Eight,
        );
        assert!(merch.is_none());
        assert!(res.is_some());

        let (res, _) = classify(
            5,
            19.0,
            72.0,
            &tags(&[("addr:housenumber", "12"), ("addr:street", "MG Road")]),
            Resolution::Eight,
        );
        assert!(res.is_some());

        let (res, merch) = classify(
            6,
            19.0,
            72.0,
            &tags(&[("shop", "bakery"), ("addr:street", "MG Road")]),
            Resolution::Eight,
        );
        assert!(res.is_none(), "merchant nodes are not also residential");
        assert!(merch.is_some());
    }
}
