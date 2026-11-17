//! Seed lookup tables: raw state names → canonical, and OSM shop categories →
//! scrip's `merchant_category` + `risk_level`.

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

const STATE_MAP_CSV: &str = include_str!("../../config/reference/ref_state_map.csv");
const CATEGORY_MAP_CSV: &str = include_str!("../../config/reference/ref_category_map.csv");

pub struct Seeds {
    /// UPPERCASED raw state name → canonical name.
    state_map: HashMap<String, String>,
    /// lowercased OSM sub-category → (merchant_category, risk_level).
    category_map: HashMap<String, (String, String)>,
}

impl Seeds {
    pub fn load(
        state_path: Option<&Path>,
        category_path: Option<&Path>,
    ) -> Result<Self, Box<dyn Error>> {
        let state_csv = match state_path {
            Some(path) => std::fs::read_to_string(path)?,
            None => STATE_MAP_CSV.to_string(),
        };
        let category_csv = match category_path {
            Some(path) => std::fs::read_to_string(path)?,
            None => CATEGORY_MAP_CSV.to_string(),
        };

        Ok(Self {
            state_map: parse_state_map(&state_csv),
            category_map: parse_category_map(&category_csv),
        })
    }

    /// Canonicalize a raw `addr:state` value; returns the trimmed raw value when
    /// the map has no entry.
    pub fn normalize_state(&self, raw: &str) -> Option<String> {
        let key = raw.trim().to_uppercase();
        if key.is_empty() {
            return None;
        }
        self.state_map
            .get(&key)
            .cloned()
            .or_else(|| Some(raw.trim().to_string()))
    }

    /// Map an OSM `sub_category` to `(merchant_category, risk_level)`, defaulting
    /// to `GENERAL_RETAIL`/`LOW`.
    pub fn category(&self, sub_category: &str) -> (&str, &str) {
        match self.category_map.get(&sub_category.trim().to_lowercase()) {
            Some((category, risk)) => (category.as_str(), risk.as_str()),
            None => ("GENERAL_RETAIL", "LOW"),
        }
    }
}

fn parse_state_map(csv: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in csv_lines(csv) {
        let fields = split_csv(line);
        if fields.len() < 2 || fields[0].eq_ignore_ascii_case("raw_name") {
            continue;
        }
        let raw = fields[0].trim().to_uppercase();
        let clean = fields[1].trim();
        if !raw.is_empty() && !clean.is_empty() {
            map.insert(raw, clean.to_string());
        }
    }
    map
}

fn parse_category_map(csv: &str) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for line in csv_lines(csv) {
        let fields = split_csv(line);
        if fields.len() < 3 || fields[0].eq_ignore_ascii_case("sub_category") {
            continue;
        }
        let sub = fields[0].trim().to_lowercase();
        if !sub.is_empty() {
            map.insert(
                sub,
                (fields[1].trim().to_string(), fields[2].trim().to_string()),
            );
        }
    }
    map
}

fn csv_lines(csv: &str) -> impl Iterator<Item = &str> {
    csv.lines()
        .map(|line| line.trim_start_matches('\u{feff}'))
        .filter(|line| !line.trim().is_empty())
}

/// Minimal CSV field splitter that honours double-quoted fields (enough for the
/// bundled seeds; no escaped-quote handling needed).
fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for ch in line.chars() {
        match ch {
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    fields.push(current);

    fields
        .into_iter()
        .map(|field| field.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_resolve_and_default() {
        let seeds = Seeds::load(None, None).unwrap();
        assert_eq!(seeds.normalize_state("TN").as_deref(), Some("Tamil Nadu"));
        assert_eq!(
            seeds.normalize_state("  maharastra ").as_deref(),
            Some("Maharashtra")
        );
        assert_eq!(seeds.normalize_state("Nowhere").as_deref(), Some("Nowhere"));
        assert_eq!(seeds.category("restaurant"), ("FOOD_AND_DRINK", "LOW"));
        assert_eq!(seeds.category("jewelry"), ("LUXURY", "VERY_HIGH"));
        assert_eq!(seeds.category("unmapped-thing"), ("GENERAL_RETAIL", "LOW"));
    }

    #[test]
    fn quoted_csv_fields_are_parsed() {
        let fields = split_csv("\"Punjab, India\", Punjab");
        assert_eq!(
            fields,
            vec!["Punjab, India".to_string(), "Punjab".to_string()]
        );
    }
}
