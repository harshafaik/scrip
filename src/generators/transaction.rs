use crate::config::{AppConfig, SessionsConfig};
use crate::core::arrival::{ArrivalProcess, WeightedArrival};
use crate::core::distribution::exponential;
use crate::core::sample::weighted_index;
use crate::generators::fraud;
use crate::generators::merchants::{MerchantRefs, SpatialIndices};
use crate::generators::net::ipv4;
use crate::models::card::Card;
use crate::models::customer::Customer;
use crate::models::fraud_metadata::FraudMetadata;
use crate::models::transaction::{Locale, Transaction};
use chrono::{DateTime, Duration, Utc};
use h3o::{CellIndex, Resolution};
use rand::{rngs::StdRng, Rng, SeedableRng};
use rayon::prelude::*;
use std::collections::HashMap;
use std::str::FromStr;

/// Generate transactions for a set of cards.
///
/// `base_end_date` and `window_days` define the timestamp range so the output is
/// reproducible (no wall-clock reads).
pub fn generate_transactions_chunk(
    cards: &[Card],
    customer_map: &HashMap<String, &Customer>,
    spatial_indices: &SpatialIndices,
    merchants: &MerchantRefs,
    config: &AppConfig,
    base_end_date: DateTime<Utc>,
    window_days: i64,
) -> (Vec<Transaction>, Vec<FraudMetadata>) {
    let h3_indices = &merchants.h3_index;
    let names = &merchants.name;
    let lats = &merchants.lat;
    let lons = &merchants.lon;
    let categories = &merchants.category;
    let osm_ids = &merchants.osm_id;
    let (index_res6, index_res4, index_state) = spatial_indices;
    let ref_count = h3_indices.len();

    let mut mcc_map: HashMap<String, String> = HashMap::new();
    for entry in &config.transactions.transactions.merchant_categories {
        mcc_map.insert(entry.name.clone(), entry.mcc.clone());
    }

    let arrival = WeightedArrival {
        hourly_weights: config
            .transactions
            .transactions
            .temporal_patterns
            .hourly_weights
            .clone(),
        daily_weights: config
            .transactions
            .transactions
            .temporal_patterns
            .daily_weights
            .clone(),
    };

    let window_days = window_days.max(1);
    let base_start_date = crate::core::time::window_start(base_end_date, window_days);

    let channel_names: Vec<String> = config.rules.payment_channels.keys().cloned().collect();
    let channel_weights: Vec<f64> = config
        .rules
        .payment_channels
        .values()
        .map(|c| c.market_share)
        .collect();
    let total_share: f64 = channel_weights.iter().sum();

    let locale = Locale {
        currency: config.rules.global.base_currency.clone(),
        country: config.rules.global.default_country.clone(),
    };

    let results: Vec<(Vec<Transaction>, Vec<FraudMetadata>)> = cards
        .par_iter()
        .map(|card| {
            let mut local_txs = Vec::new();
            let mut local_meta = Vec::new();

            let mut card_rng = StdRng::seed_from_u64(
                config.rules.global.seed as u64
                    + config.tuning.salts.injector as u64
                    + card
                        .card_id
                        .as_bytes()
                        .iter()
                        .map(|&b| b as u64)
                        .sum::<u64>(),
            );

            let customer = customer_map
                .get(&card.customer_id)
                .expect("Customer missing");
            let cust_cell = CellIndex::from_str(&customer.location.home_h3r7).expect("H3 invalid");

            let p6_key = cust_cell.parent(Resolution::Six).unwrap().to_string();
            let p4_key = cust_cell.parent(Resolution::Four).unwrap().to_string();
            let state_key = &customer.location.state;

            let annual_budget = customer.financial.monthly_spend * 12.0;

            let mut device_map: HashMap<String, String> = HashMap::new();
            for (channel, chan_config) in &config.rules.payment_channels {
                if !chan_config.user_agents.is_empty() {
                    let idx = card_rng.random_range(0..chan_config.user_agents.len());
                    device_map.insert(channel.clone(), chan_config.user_agents[idx].clone());
                }
            }

            let campaign_ctx = fraud::initialize_campaign(config, &mut card_rng, &card.card_id);

            let num_txns = config
                .customer
                .control
                .transactions_per_customer
                .sample(&mut card_rng)
                .max(1);
            let avg_txn_amount = annual_budget / num_txns as f64;

            // --- Step 1: Pre-generate and Sort Timestamps ---
            let timestamps = generate_timestamps(
                num_txns,
                base_start_date,
                window_days,
                &arrival,
                &config.transactions.transactions.sessions,
                &mut card_rng,
            );

            let mut last_processed_date = None;

            for i in 0..num_txns {
                // 80% super-local (res 6), 15% city (res 4), 3% state, 2% global
                let r_spatial: f64 = card_rng.random();
                let idx = if r_spatial < 0.80 {
                    index_res6
                        .get(&p6_key)
                        .map(|v| v[card_rng.random_range(0..v.len())])
                        .unwrap_or_else(|| card_rng.random_range(0..ref_count))
                } else if r_spatial < 0.95 {
                    index_res4
                        .get(&p4_key)
                        .map(|v| v[card_rng.random_range(0..v.len())])
                        .unwrap_or_else(|| card_rng.random_range(0..ref_count))
                } else if r_spatial < 0.98 {
                    index_state
                        .get(state_key)
                        .map(|v| v[card_rng.random_range(0..v.len())])
                        .unwrap_or_else(|| card_rng.random_range(0..ref_count))
                } else {
                    card_rng.random_range(0..ref_count)
                };

                let tx_id = format!(
                    "tx_{}_{}_{}",
                    &card.card_id[..8],
                    i,
                    card_rng.random_range(1000..9999)
                );

                let effective_share = if customer.financial.is_fraud {
                    config.rules.fraud_injector.target_share
                        * config.rules.fraud_injector.customer_fraud_multiplier
                } else {
                    config.rules.fraud_injector.target_share
                };
                let (fraud_target, is_fraud_label, label_noise) = fraud::determine_targeting(
                    effective_share,
                    config.rules.fraud_injector.default_fn_rate,
                    config.rules.fraud_injector.default_fp_rate,
                    &mut card_rng,
                );

                let tx_date = timestamps[i];
                let r_chan: f64 = card_rng.random_range(0.0..total_share);
                let final_channel = weighted_index(r_chan, &channel_weights)
                    .map(|i| channel_names[i].clone())
                    .unwrap_or_else(|| "upi".to_string());

                let final_ua = {
                    let ua_roll = card_rng.random_range(0.0..1.0);
                    if ua_roll < 0.92 {
                        customer.device.primary_ua.clone()
                    } else if ua_roll < 0.98 {
                        customer
                            .device
                            .secondary_ua
                            .clone()
                            .unwrap_or_else(|| customer.device.primary_ua.clone())
                    } else {
                        let pools = [
                            &config.customer.device_profiles.android_ua_pool,
                            &config.customer.device_profiles.ios_ua_pool,
                            &config.customer.device_profiles.upi_app_ua_pool,
                        ];
                        let pool = pools[card_rng.random_range(0..pools.len())];
                        pool[card_rng.random_range(0..pool.len())].clone()
                    }
                };

                let final_ip = {
                    let ip_roll = card_rng.random_range(0.0..1.0);
                    if ip_roll < 0.85 {
                        ipv4(&customer.device.ip_subnet, &mut card_rng)
                    } else if ip_roll < 0.95 {
                        let isp_subnets: Vec<&String> =
                            config.customer.isp_assignment.subnets.values().collect();
                        let random_subnet =
                            isp_subnets[card_rng.random_range(0..isp_subnets.len())];
                        ipv4(random_subnet, &mut card_rng)
                    } else if ip_roll < 0.99 {
                        let vpn_prefixes = ["185.16.", "104.16."];
                        ipv4(
                            vpn_prefixes[card_rng.random_range(0..vpn_prefixes.len())],
                            &mut card_rng,
                        )
                    } else {
                        ipv4("", &mut card_rng)
                    }
                };
                let mut final_amount =
                    (avg_txn_amount * card_rng.random_range(0.4..1.6)).clamp(1.0, 500_000.0);

                let jitter_lat = card_rng.random_range(-0.001..0.001);
                let jitter_lon = card_rng.random_range(-0.001..0.001);
                let final_lat = lats[idx] + jitter_lat;
                let final_lon = lons[idx] + jitter_lon;

                let final_card_present =
                    card_rng.random_bool(config.transactions.transactions.card_present_probability);

                let cat_name = &categories[idx];

                if !fraud_target {
                    let fat_tail_categories =
                        ["ELECTRONICS", "TRAVEL", "MEDICAL", "JEWELLERY", "EDUCATION"];
                    if fat_tail_categories.contains(&cat_name.as_str()) {
                        if card_rng.random_bool(0.04) {
                            final_amount = (avg_txn_amount * card_rng.random_range(2.0..8.0))
                                .clamp(1.0, 500_000.0);
                        }
                    }
                }

                let mcc = mcc_map.get(cat_name).unwrap_or(&"5999".to_string()).clone();
                let merchant = crate::models::transaction::MerchantInfo {
                    id: osm_ids[idx].to_string(),
                    name: names[idx].clone(),
                    category: cat_name.clone(),
                    mcc,
                    lat: final_lat,
                    long: final_lon,
                    h3_r7: h3_indices[idx].clone(),
                };

                let mut tx = Transaction::new(
                    tx_id.clone(),
                    card.card_id.clone(),
                    card.account_id.clone(),
                    card.customer_id.clone(),
                    merchant,
                    final_amount,
                    tx_date,
                    final_channel,
                    final_ua,
                    final_ip,
                    ("Success".to_string(), "approved".to_string(), None),
                    is_fraud_label,
                    final_card_present,
                    &locale,
                );

                if let Some(meta) = fraud::inject_fraud(
                    &mut tx,
                    fraud_target,
                    is_fraud_label,
                    label_noise,
                    &campaign_ctx,
                    last_processed_date,
                    avg_txn_amount,
                    i,
                    config,
                    &mut card_rng,
                ) {
                    local_meta.push(meta);
                }

                if let Ok(parsed) = DateTime::parse_from_rfc3339(&tx.timestamp) {
                    if parsed.with_timezone(&Utc) >= base_end_date {
                        tx.timestamp = (base_end_date - Duration::seconds(1)).to_rfc3339();
                    }
                }

                last_processed_date = Some(
                    DateTime::parse_from_rfc3339(&tx.timestamp)
                        .unwrap()
                        .with_timezone(&Utc),
                );
                local_txs.push(tx);
            }

            (local_txs, local_meta)
        })
        .collect();

    let mut all_txs = Vec::new();
    let mut all_meta = Vec::new();
    for (mut txs, mut meta) in results {
        all_txs.append(&mut txs);
        all_meta.append(&mut meta);
    }
    (all_txs, all_meta)
}

fn generate_timestamps(
    num_txns: usize,
    base_start_date: DateTime<Utc>,
    window_days: i64,
    arrival: &WeightedArrival,
    sessions: &SessionsConfig,
    rng: &mut StdRng,
) -> Vec<DateTime<Utc>> {
    let window_seconds = window_days.max(1) * 86_400;
    let sizes = sample_session_sizes(num_txns, &sessions.size_weights, rng);
    let starts = arrival.sample_starts(sizes.len(), window_seconds, rng);
    let mean_iet = sessions.within_iet_mean_seconds.max(1.0);

    let mut timestamps = Vec::with_capacity(num_txns);
    for (start, size) in starts.into_iter().zip(sizes) {
        let mut offsets = Vec::with_capacity(size);
        let mut offset = 0.0;
        for i in 0..size {
            if i > 0 {
                offset += exponential(mean_iet, rng);
            }
            offsets.push(offset);
        }

        let span = offsets.last().copied().unwrap_or(0.0);
        let latest_start = (window_seconds as f64 - span).max(0.0);
        let start = (start as f64).min(latest_start).max(0.0);
        for offset in offsets {
            let seconds = (start + offset).min((window_seconds - 1) as f64).max(0.0) as i64;
            timestamps.push(base_start_date + Duration::seconds(seconds));
        }
    }

    timestamps.truncate(num_txns);
    while timestamps.len() < num_txns {
        timestamps.push(*timestamps.last().unwrap_or(&base_start_date));
    }
    timestamps.sort();
    timestamps
}

fn sample_session_sizes(total: usize, weights: &[f64], rng: &mut StdRng) -> Vec<usize> {
    if total == 0 {
        return Vec::new();
    }
    let total_weight: f64 = weights.iter().filter(|weight| **weight > 0.0).sum();
    if weights.is_empty() || total_weight <= 0.0 {
        return vec![1; total];
    }

    let mut sizes = Vec::new();
    let mut remaining = total;
    while remaining > 0 {
        let draw = rng.random::<f64>() * total_weight;
        let index = weighted_index(draw, weights).unwrap_or(0);
        let size = (index + 1).min(remaining);
        sizes.push(size);
        remaining -= size;
    }
    sizes
}
