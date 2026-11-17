use crate::reference::Merchants;
use h3o::{CellIndex, Resolution};
use std::collections::HashMap;
use std::str::FromStr;

/// Merchant reference data held column-wise, ready for positional lookup during
/// transaction generation.
pub struct MerchantRefs {
    pub h3_index: Vec<String>,
    pub name: Vec<String>,
    pub lat: Vec<f64>,
    pub lon: Vec<f64>,
    pub category: Vec<String>,
    pub osm_id: Vec<i64>,
    pub state: Vec<String>,
}

/// Spatial lookups over merchants: by H3 res-6 parent, res-4 parent, and state.
pub type SpatialIndices = (
    HashMap<String, Vec<usize>>,
    HashMap<String, Vec<usize>>,
    HashMap<String, Vec<usize>>,
);

/// Build the columnar [`MerchantRefs`] plus the spatial indices used to pick
/// merchants near a customer.
pub fn build_merchant_index(m: Merchants) -> (MerchantRefs, SpatialIndices) {
    let refs = MerchantRefs {
        h3_index: m.h3_index,
        name: m.merchant_name,
        lat: m.latitude,
        lon: m.longitude,
        category: m.merchant_category,
        osm_id: m.osm_id,
        state: m.state,
    };

    let mut index_res6: HashMap<String, Vec<usize>> = HashMap::new();
    let mut index_res4: HashMap<String, Vec<usize>> = HashMap::new();
    let mut index_state: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, h3_str) in refs.h3_index.iter().enumerate() {
        if let Ok(cell) = CellIndex::from_str(h3_str) {
            index_res6
                .entry(cell.parent(Resolution::Six).unwrap().to_string())
                .or_default()
                .push(idx);
            index_res4
                .entry(cell.parent(Resolution::Four).unwrap().to_string())
                .or_default()
                .push(idx);
        }
        index_state
            .entry(refs.state[idx].clone())
            .or_default()
            .push(idx);
    }

    (refs, (index_res6, index_res4, index_state))
}
