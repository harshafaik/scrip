//! Label normalization: pincode digit-cleaning and the best-effort city cleanup
//! ported from RiskFabric's `normalize_city` dbt macro.

/// Keep only digits; returns `None` when nothing remains.
pub fn pincode(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

/// Best-effort city label cleanup: strip one trailing noise token, then take the
/// text before the first `-`, then before the first `,`, then trim + uppercase.
pub fn city(raw: &str) -> Option<String> {
    let mut value = raw.trim();

    let lower = value.to_lowercase();
    for suffix in NOISE_SUFFIXES {
        let needle = format!(" {}", suffix.to_lowercase());
        if lower.ends_with(&needle) {
            value = &value[..value.len() - needle.len()];
            break;
        }
    }

    let value = value.split('-').next().unwrap_or("");
    let value = value.split(',').next().unwrap_or("");
    let value = value.trim().to_uppercase();

    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Trailing tokens the RiskFabric macro strips (India-specific).
const NOISE_SUFFIXES: &[&str] = &[
    "INDIA",
    "MAHARASHTRA",
    "TELANGANA",
    "KARNATAKA",
    "TAMIL NADU",
    "PUNE",
    "HYDERABAD",
    "BENGALURU",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pincode_keeps_digits_only() {
        assert_eq!(pincode("400 001"), Some("400001".to_string()));
        assert_eq!(pincode("KA-560001"), Some("560001".to_string()));
        assert_eq!(pincode("none"), None);
    }

    #[test]
    fn city_matches_macro_semantics() {
        assert_eq!(city("Mumbai, Maharashtra").as_deref(), Some("MUMBAI"));
        assert_eq!(city("Pune-411001").as_deref(), Some("PUNE"));
        assert_eq!(city("Bengaluru India").as_deref(), Some("BENGALURU"));
        assert_eq!(city("  "), None);
    }
}
