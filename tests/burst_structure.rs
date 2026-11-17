//! T1 acceptance: time is a session process, not a sorted marginal.
//!
//! Bands are read from `transactions.sessions` in the embedded config. They were
//! derived from `~/.hermes/cache/scratch/t1_probe.py` with:
//!   size_weights = [0.70, 0.25, 0.04, 0.01]
//!   within_iet_mean_seconds = 300
//!   session starts = hour-weighted on uniform days
//! Probe output for those parameters:
//!   25c x 30d x 300tx: iid ac1 -0.007, Fano 1.21, burst-mem 29.3%
//!                      sess ac1 -0.038, Fano 1.77, burst-mem 48.6%
//!   25c x 365d x 75tx: iid ac1 -0.020, Fano 1.00, burst-mem 0.0%
//!                      sess ac1 -0.065, Fano 1.56, burst-mem 11.5%
//! This generator also applies day weights; observed (seed 42):
//!   standard ac1 -0.017, Fano 1.79, within p50 187s, inter p50 16.6ks
//!   sparse burst membership 12.24%
//! Re-run the probe and update the config bands whenever these parameters change.
//! The dense 25c/30d fixture is intentionally excluded from the burst-band check.

use chrono::{DateTime, Utc};
use scrip::config::{AppConfig, RangeControl};
use scrip::fixtures;
use scrip::generators::run::{generate_all, GenParams};
use scrip::metrics;
use std::collections::BTreeMap;

fn run(cfg: &AppConfig, customers: usize, days: i64, seed: u64) -> scrip::Dataset {
    let tmp = tempfile::tempdir().unwrap();
    let refs = tmp.path().join("refs");
    fixtures::write_reference_fixtures(&refs).unwrap();

    let base = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let params = GenParams {
        seed,
        customers,
        days,
        base_end_date: base,
        input_dir: refs,
        output_dir: tmp.path().join("out"),
    };
    generate_all(cfg, &params).unwrap()
}

#[test]
fn standard_fixture_has_session_structure() {
    let cfg = AppConfig::default_embedded();
    let sessions = cfg.transactions.transactions.sessions.clone();
    let data = run(&cfg, 25, 30, 42);
    let report = metrics::audit_dataset(&data);
    println!("standard behavioral: {:?}", report.behavioral);

    assert!(
        report.behavioral.fano_hourly > sessions.fano_min,
        "hourly Fano {:.3} <= {:.3}",
        report.behavioral.fano_hourly,
        sessions.fano_min
    );
    assert!(
        report.behavioral.iet_autocorr.abs() < sessions.iet_autocorr_bound,
        "|IET ac1| {:.4} >= {:.4}",
        report.behavioral.iet_autocorr.abs(),
        sessions.iet_autocorr_bound
    );
    assert!(
        report.behavioral.within_session_p50 >= sessions.within_session_p50_min_seconds
            && report.behavioral.within_session_p50 <= sessions.within_session_p50_max_seconds,
        "within-session p50 {:.1}s outside [{:.1}, {:.1}]",
        report.behavioral.within_session_p50,
        sessions.within_session_p50_min_seconds,
        sessions.within_session_p50_max_seconds
    );
    assert!(
        report.behavioral.inter_session_p50 > sessions.inter_session_p50_min_seconds,
        "inter-session p50 {:.1}s <= {:.1}s",
        report.behavioral.inter_session_p50,
        sessions.inter_session_p50_min_seconds
    );
    assert!(
        report.behavioral.iet_p90_p10_ratio > sessions.iet_p90_p10_min_ratio,
        "IET p90/p10 {:.1} <= {:.1}",
        report.behavioral.iet_p90_p10_ratio,
        sessions.iet_p90_p10_min_ratio
    );

    let range = &cfg.customer.control.transactions_per_customer;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for tx in &data.transactions {
        *counts.entry(tx.card_id.as_str()).or_default() += 1;
    }
    for (card, count) in counts {
        assert!(
            count >= range.min && count <= range.max,
            "card {card} has {count} transactions, outside [{}, {}]",
            range.min,
            range.max
        );
    }
}

#[test]
fn sparse_fixture_burst_membership_is_in_band() {
    let mut cfg = AppConfig::default_embedded();
    cfg.customer.control.transactions_per_customer = RangeControl { min: 75, max: 75 };
    let sessions = cfg.transactions.transactions.sessions.clone();

    let data = run(&cfg, 25, 365, 42);
    let report = metrics::audit_dataset(&data);
    println!("sparse behavioral: {:?}", report.behavioral);

    let burst = report.behavioral.burst_membership / 100.0;
    assert!(
        burst >= sessions.burst_membership_min && burst <= sessions.burst_membership_max,
        "burst membership {:.2}% outside [{:.1}%, {:.1}%]",
        report.behavioral.burst_membership,
        sessions.burst_membership_min * 100.0,
        sessions.burst_membership_max * 100.0
    );
}
