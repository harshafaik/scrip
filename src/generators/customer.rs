use crate::config::AppConfig;
use crate::core::sample::weighted_index;
use crate::models::customer::{Customer, DeviceProfile, FinancialProfile, GeoLocation};
use crate::reference::{self, ReferenceError};
use chrono::{DateTime, Utc};
use rand::Rng;
use rayon::prelude::*;
use std::path::Path;
use uuid::Uuid;

/// Generate `count` customers using the reference points in `residential_ref`
/// (a `ref_residential.parquet` matching the [`crate::reference`] schema).
///
/// `now` fixes the registration window so the run is reproducible.
pub fn generate_customers(
    count: usize,
    base_seed: u64,
    now: DateTime<Utc>,
    config: &AppConfig,
    residential_ref: &Path,
) -> Result<Vec<Customer>, ReferenceError> {
    let reference::ResidentialRefs {
        h3_index: h3_indices,
        latitude: lats,
        longitude: lons,
        state: states,
        city: cities,
        pincode: postcodes,
    } = reference::load_residential(residential_ref)?;

    let ref_count = h3_indices.len();

    let salt_customer: u64 = 0xDEAD_0000_0000_0001;

    let customers = (0..count)
        .into_par_iter()
        .map(|i| {
            let mut rng = crate::core::rng::stream(base_seed, salt_customer, i as u64);
            let idx = rng.random_range(0..ref_count);

            let first_name = &config.customer.names.first_names
                [rng.random_range(0..config.customer.names.first_names.len())];
            let last_name = &config.customer.names.last_names
                [rng.random_range(0..config.customer.names.last_names.len())];
            let name = format!("{} {}", first_name, last_name);

            let domain = &config.customer.email.domains
                [rng.random_range(0..config.customer.email.domains.len())];
            let email = format!(
                "{}.{}{}@{}",
                first_name.to_lowercase(),
                last_name.to_lowercase(),
                rng.random_range(10..999),
                domain
            );

            let age: u8 = rng.random_range(18..85);
            let customer_id = Uuid::from_u128(rng.random::<u128>()).to_string();

            let jitter_lat = rng.random_range(-0.005..0.005);
            let jitter_lon = rng.random_range(-0.005..0.005);
            let final_lat = lats[idx] + jitter_lat;
            let final_lon = lons[idx] + jitter_lon;

            let city_name = cities[idx].as_deref().unwrap_or("");
            let location_type = if config
                .customer
                .locations
                .metro_cities
                .iter()
                .any(|m| m.to_lowercase() == city_name.to_lowercase())
            {
                "Metro".to_string()
            } else if !city_name.is_empty() {
                "Urban".to_string()
            } else {
                let types = &config.customer.locations.types;
                types[rng.random_range(0..types.len())].clone()
            };

            let base_cs = config.customer.financials.credit_score.base as f64;
            let age_factor =
                (age as f64 - 18.0) * config.customer.financials.credit_score.age_weight;
            let noise = rng.random_range(-50.0..50.0);
            let credit_score = (base_cs + age_factor + noise).clamp(
                config.customer.financials.credit_score.min as f64,
                config.customer.financials.credit_score.max as f64,
            ) as u16;

            let base_spend = config
                .customer
                .financials
                .base_spend
                .get(&location_type)
                .copied()
                .unwrap_or(15000.0);
            let age_spend_multiplier = 1.0 - ((age as f64 - 45.0).abs() / 60.0);
            let spend_noise = rng.random_range(0.7..1.4);
            let monthly_spend = base_spend * age_spend_multiplier * spend_noise;

            let is_fraud = rng.random_bool(0.02);
            let customer_risk_score = if is_fraud {
                rng.random_range(0.6..0.99)
            } else {
                rng.random_range(0.01..0.6)
            } as f32;

            let shares = config
                .customer
                .device_profiles
                .location_shares
                .get(&location_type)
                .or_else(|| config.customer.device_profiles.location_shares.get("Urban"))
                .expect("Missing device profile shares");

            let ua_roll = rng.random_range(0.0..1.0);
            let primary_ua = if ua_roll < shares.android_share {
                config.customer.device_profiles.android_ua_pool
                    [rng.random_range(0..config.customer.device_profiles.android_ua_pool.len())]
                .clone()
            } else if ua_roll < shares.android_share + shares.ios_share {
                config.customer.device_profiles.ios_ua_pool
                    [rng.random_range(0..config.customer.device_profiles.ios_ua_pool.len())]
                .clone()
            } else if ua_roll < shares.android_share + shares.ios_share + shares.upi_app_share {
                config.customer.device_profiles.upi_app_ua_pool
                    [rng.random_range(0..config.customer.device_profiles.upi_app_ua_pool.len())]
                .clone()
            } else {
                config.customer.device_profiles.desktop_ua_pool
                    [rng.random_range(0..config.customer.device_profiles.desktop_ua_pool.len())]
                .clone()
            };

            let secondary_ua = if rng.random_bool(0.15) {
                let ua_roll_sec = rng.random_range(0.0..1.0);
                Some(if ua_roll_sec < 0.7 {
                    config.customer.device_profiles.android_ua_pool
                        [rng.random_range(0..config.customer.device_profiles.android_ua_pool.len())]
                    .clone()
                } else {
                    config.customer.device_profiles.upi_app_ua_pool
                        [rng.random_range(0..config.customer.device_profiles.upi_app_ua_pool.len())]
                    .clone()
                })
            } else {
                None
            };

            let isp_shares = config
                .customer
                .isp_assignment
                .shares
                .get(&location_type)
                .or_else(|| config.customer.isp_assignment.shares.get("Urban"))
                .expect("Missing ISP shares");

            let isp_weights: Vec<f64> = isp_shares.values().copied().collect();
            let isp_roll = rng.random_range(0.0..1.0);
            let selected_isp = weighted_index(isp_roll, &isp_weights)
                .map(|i| isp_shares.keys().nth(i).unwrap().clone())
                .unwrap_or_else(|| "Others".to_string());

            let ip_subnet = config
                .customer
                .isp_assignment
                .subnets
                .get(&selected_isp)
                .cloned()
                .unwrap_or_else(|| "103.0.0.x/16".to_string());

            Customer::new(
                customer_id,
                name,
                age,
                email,
                GeoLocation {
                    location: "".to_string(),
                    city: cities[idx].clone(),
                    state: states[idx].clone(),
                    location_type,
                    postcode: postcodes[idx].clone(),
                    home_latitude: final_lat,
                    home_longitude: final_lon,
                    home_h3r5: "".to_string(),
                    home_h3r7: h3_indices[idx].clone(),
                },
                FinancialProfile {
                    credit_score,
                    monthly_spend,
                    customer_risk_score,
                    is_fraud,
                },
                DeviceProfile {
                    primary_ua,
                    secondary_ua,
                    isp: selected_isp,
                    ip_subnet,
                },
                &mut rng,
                now,
                config.customer.registration.lookback_years,
            )
        })
        .collect();

    Ok(customers)
}
