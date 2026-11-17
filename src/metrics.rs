//! Behavioral, statistical, and leakage audit for a generated dataset.
//!
//! The metric definitions mirror the T1 probe (`t1_probe.py`): per-card IET
//! autocorrelation, hourly-count Fano factor, transaction-level burst
//! membership, and the canonical velocity rules. The audit is a scoreboard, not
//! a gate; T1's tests assert against bands derived from the same definitions.

use crate::models::dataset::Dataset;
use chrono::{DateTime, Timelike};
use polars::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default)]
pub struct ClassRates {
    pub fraud: f64,
    pub legit: f64,
}

#[derive(Debug, Clone, Default)]
pub struct StatisticalReport {
    pub customers: usize,
    pub accounts: usize,
    pub cards: usize,
    pub transactions: usize,
    pub fraud_target_rate: f64,
    pub noisy_fraud_rate: f64,
    pub amount_p50: f64,
    pub amount_p90: f64,
    pub amount_p99: f64,
    pub amount_skew: f64,
}

#[derive(Debug, Clone, Default)]
pub struct BehavioralReport {
    pub iet_p50: f64,
    pub iet_p90: f64,
    pub iet_p99: f64,
    pub iet_autocorr: f64,
    pub fano_hourly: f64,
    pub burst_membership: f64,
    pub cards_with_burst: f64,
    pub velocity_3_1h: ClassRates,
    pub velocity_5_24h: ClassRates,
    pub amount_z_gt_3: ClassRates,
    pub same_merchant_10m: ClassRates,
    pub within_session_p50: f64,
    pub inter_session_p50: f64,
    pub iet_p90_p10_ratio: f64,
}

#[derive(Debug, Clone, Default)]
pub struct LeakageReport {
    pub features: Vec<(String, Option<f64>)>,
}

#[derive(Debug, Clone, Default)]
pub struct AuditReport {
    pub input: String,
    pub statistical: StatisticalReport,
    pub behavioral: BehavioralReport,
    pub leakage: LeakageReport,
}

#[derive(Debug, Clone)]
struct Sample {
    card_id: String,
    ts: i64,
    amount: f64,
    merchant_id: String,
    fraud_target: bool,
    noisy_fraud: bool,
    card_present: bool,
    channel: String,
    category: String,
}

#[derive(Default)]
struct RuleCounts {
    fraud_hits: usize,
    fraud_total: usize,
    legit_hits: usize,
    legit_total: usize,
}

impl RuleCounts {
    fn add(&mut self, fraud: bool, hit: bool) {
        if fraud {
            self.fraud_total += 1;
            self.fraud_hits += usize::from(hit);
        } else {
            self.legit_total += 1;
            self.legit_hits += usize::from(hit);
        }
    }

    fn rates(&self) -> ClassRates {
        ClassRates {
            fraud: pct(self.fraud_hits, self.fraud_total),
            legit: pct(self.legit_hits, self.legit_total),
        }
    }
}

/// Audit an in-memory dataset (used by tests).
pub fn audit_dataset(ds: &Dataset) -> AuditReport {
    let targets: HashMap<&str, bool> = ds
        .fraud_metadata
        .iter()
        .map(|m| (m.transaction_id.as_str(), m.fraud_target))
        .collect();

    let samples = ds
        .transactions
        .iter()
        .filter_map(|t| {
            let ts = parse_ts(&t.timestamp)?;
            Some(Sample {
                card_id: t.card_id.clone(),
                ts,
                amount: t.amount,
                merchant_id: t.merchant_id.clone(),
                fraud_target: targets
                    .get(t.transaction_id.as_str())
                    .copied()
                    .unwrap_or(false),
                noisy_fraud: t.is_fraud,
                card_present: t.card_present,
                channel: t.transaction_channel.clone(),
                category: t.merchant_category.clone(),
            })
        })
        .collect();

    audit_samples(
        samples,
        ds.customers.len(),
        ds.accounts.len(),
        ds.cards.len(),
        "dataset".to_string(),
    )
}

/// Audit a generated output directory (`customers.parquet`, etc.).
pub fn audit_dir(dir: &Path) -> Result<AuditReport, Box<dyn Error>> {
    let tx = read_parquet_df(&dir.join("transactions.parquet"))?;
    let meta = read_parquet_df(&dir.join("fraud_metadata.parquet"))?;
    let customers = read_parquet_df(&dir.join("customers.parquet"))?;
    let accounts = read_parquet_df(&dir.join("accounts.parquet"))?;
    let cards = read_parquet_df(&dir.join("cards.parquet"))?;

    let targets = target_map(&meta)?;

    let n = tx.height();
    let ids = string_values(&tx, "transaction_id")?;
    let card_ids = string_values(&tx, "card_id")?;
    let timestamps = timestamp_values(&tx, "timestamp")?;
    let amounts = f64_values(&tx, "amount")?;
    let merchants = string_values(&tx, "merchant_id")?;
    let noisy = bool_values(&tx, "is_fraud")?;
    let present = bool_values(&tx, "card_present")?;
    let channels = string_values(&tx, "transaction_channel")?;
    let categories = string_values(&tx, "merchant_category")?;

    if ids.len() != n
        || card_ids.len() != n
        || timestamps.len() != n
        || amounts.len() != n
        || merchants.len() != n
        || noisy.len() != n
        || present.len() != n
        || channels.len() != n
        || categories.len() != n
    {
        return Err("transactions.parquet has mismatched column lengths".into());
    }

    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        let (
            Some(id),
            Some(card),
            Some(ts),
            Some(amount),
            Some(merchant),
            Some(noisy),
            Some(present),
        ) = (
            ids[i].clone(),
            card_ids[i].clone(),
            timestamps[i],
            amounts[i],
            merchants[i].clone(),
            noisy[i],
            present[i],
        )
        else {
            continue;
        };

        samples.push(Sample {
            card_id: card,
            ts,
            amount,
            merchant_id: merchant,
            fraud_target: targets.get(&id).copied().unwrap_or(false),
            noisy_fraud: noisy,
            card_present: present,
            channel: channels[i].clone().unwrap_or_default(),
            category: categories[i].clone().unwrap_or_default(),
        });
    }

    Ok(audit_samples(
        samples,
        customers.height(),
        accounts.height(),
        cards.height(),
        dir.display().to_string(),
    ))
}

fn audit_samples(
    samples: Vec<Sample>,
    customers: usize,
    accounts: usize,
    cards: usize,
    input: String,
) -> AuditReport {
    let total = samples.len();
    let mut report = AuditReport {
        input,
        ..AuditReport::default()
    };
    report.statistical.customers = customers;
    report.statistical.accounts = accounts;
    report.statistical.cards = cards;
    report.statistical.transactions = total;

    if total == 0 {
        return report;
    }

    let fraud_targets = samples.iter().filter(|s| s.fraud_target).count();
    let noisy_frauds = samples.iter().filter(|s| s.noisy_fraud).count();
    report.statistical.fraud_target_rate = pct(fraud_targets, total);
    report.statistical.noisy_fraud_rate = pct(noisy_frauds, total);

    let mut amounts: Vec<f64> = samples.iter().map(|s| s.amount).collect();
    amounts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    report.statistical.amount_p50 = percentile(&amounts, 0.50);
    report.statistical.amount_p90 = percentile(&amounts, 0.90);
    report.statistical.amount_p99 = percentile(&amounts, 0.99);
    report.statistical.amount_skew = skewness(&amounts);

    let min_ts = samples.iter().map(|s| s.ts).min().unwrap();
    let max_ts = samples.iter().map(|s| s.ts).max().unwrap();
    let buckets = (((max_ts - min_ts) / 3600) + 1).max(1) as usize;

    let mut by_card: BTreeMap<&str, Vec<&Sample>> = BTreeMap::new();
    for sample in &samples {
        by_card
            .entry(sample.card_id.as_str())
            .or_default()
            .push(sample);
    }

    let mut iets: Vec<f64> = Vec::new();
    let mut autocorrs: Vec<f64> = Vec::new();
    let mut fanos: Vec<f64> = Vec::new();
    let mut cards_with_burst = 0usize;
    let mut burst_member_total = 0usize;
    let mut rule_3_1h = RuleCounts::default();
    let mut rule_5_24h = RuleCounts::default();
    let mut rule_z = RuleCounts::default();
    let mut rule_repeat = RuleCounts::default();

    for card_samples in by_card.values() {
        let mut ordered: Vec<&Sample> = card_samples.clone();
        ordered.sort_by_key(|s| s.ts);
        let times: Vec<i64> = ordered.iter().map(|s| s.ts).collect();

        let card_iets: Vec<f64> = times.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
        iets.extend(card_iets.iter().copied());
        if let Some(autocorr) = lag1_autocorr(&card_iets) {
            autocorrs.push(autocorr);
        }
        if let Some(fano) = fano(&times, min_ts, buckets) {
            fanos.push(fano);
        }

        let members_3 = burst_members(&times, 3600, 3);
        let members_5 = burst_members(&times, 86400, 5);
        let repeats = same_merchant_hits(&ordered, &times);
        let z_hits = amount_z_hits(&ordered);

        if members_3.iter().any(|&m| m) {
            cards_with_burst += 1;
        }
        for (i, sample) in ordered.iter().enumerate() {
            if members_3[i] {
                burst_member_total += 1;
            }
            rule_3_1h.add(sample.fraud_target, members_3[i]);
            rule_5_24h.add(sample.fraud_target, members_5[i]);
            rule_repeat.add(sample.fraud_target, repeats[i]);
            rule_z.add(sample.fraud_target, z_hits[i]);
        }
    }

    report.behavioral.burst_membership = pct(burst_member_total, total);
    report.behavioral.cards_with_burst = pct(cards_with_burst, by_card.len());
    report.behavioral.velocity_3_1h = rule_3_1h.rates();
    report.behavioral.velocity_5_24h = rule_5_24h.rates();
    report.behavioral.amount_z_gt_3 = rule_z.rates();
    report.behavioral.same_merchant_10m = rule_repeat.rates();

    report.behavioral.iet_autocorr = mean(&autocorrs);
    report.behavioral.fano_hourly = mean(&fanos);

    iets.sort_by(|a, b| a.partial_cmp(b).unwrap());
    report.behavioral.iet_p50 = percentile(&iets, 0.50);
    report.behavioral.iet_p90 = percentile(&iets, 0.90);
    report.behavioral.iet_p99 = percentile(&iets, 0.99);
    let p10 = percentile(&iets, 0.10);
    report.behavioral.iet_p90_p10_ratio = if p10 > 0.0 {
        report.behavioral.iet_p90 / p10
    } else {
        f64::INFINITY
    };

    let mut within: Vec<f64> = iets.iter().copied().filter(|&x| x <= 600.0).collect();
    let mut inter: Vec<f64> = iets.iter().copied().filter(|&x| x >= 7200.0).collect();
    within.sort_by(|a, b| a.partial_cmp(b).unwrap());
    inter.sort_by(|a, b| a.partial_cmp(b).unwrap());
    report.behavioral.within_session_p50 = percentile(&within, 0.50);
    report.behavioral.inter_session_p50 = percentile(&inter, 0.50);

    let mut features = Vec::new();
    features.push(("amount".to_string(), auc_feature(&samples, |s| s.amount)));
    features.push((
        "hour_of_day".to_string(),
        auc_feature(&samples, |s| hour_of_day(s.ts)),
    ));
    features.push((
        "is_night".to_string(),
        auc_feature(&samples, |s| {
            let hour = hour_of_day(s.ts);
            if (6.0..22.0).contains(&hour) {
                0.0
            } else {
                1.0
            }
        }),
    ));
    features.push((
        "card_present".to_string(),
        auc_feature(&samples, |s| if s.card_present { 1.0 } else { 0.0 }),
    ));
    features.push((
        "transaction_channel".to_string(),
        categorical_auc(&samples, |s| s.channel.as_str()),
    ));
    features.push((
        "merchant_category".to_string(),
        categorical_auc(&samples, |s| s.category.as_str()),
    ));
    report.leakage.features = features;

    report
}

pub fn print_report(report: &AuditReport) {
    println!("=== scrip audit: {} ===", report.input);
    println!("statistical");
    println!(
        "  customers/accounts/cards/transactions = {} / {} / {} / {}",
        report.statistical.customers,
        report.statistical.accounts,
        report.statistical.cards,
        report.statistical.transactions
    );
    println!(
        "  fraud_target = {:.3}% | noisy is_fraud = {:.3}%",
        report.statistical.fraud_target_rate, report.statistical.noisy_fraud_rate
    );
    println!(
        "  amount p50/p90/p99 = {:.2} / {:.2} / {:.2} | skew = {:.3}",
        report.statistical.amount_p50,
        report.statistical.amount_p90,
        report.statistical.amount_p99,
        report.statistical.amount_skew
    );

    println!("behavioral");
    println!(
        "  IET p50/p90/p99 (s) = {:.0} / {:.0} / {:.0} | ac1 = {:+.4}",
        report.behavioral.iet_p50,
        report.behavioral.iet_p90,
        report.behavioral.iet_p99,
        report.behavioral.iet_autocorr
    );
    println!(
        "  Fano (hourly counts) = {:.3} | burst membership (3/60m) = {:.2}% | cards w/burst = {:.1}%",
        report.behavioral.fano_hourly,
        report.behavioral.burst_membership,
        report.behavioral.cards_with_burst
    );
    println!(
        "  within-session p50 = {:.0}s | inter-session p50 = {:.0}s | p90/p10 = {:.1}",
        report.behavioral.within_session_p50,
        report.behavioral.inter_session_p50,
        report.behavioral.iet_p90_p10_ratio
    );
    print_rule("3 txns/1h", report.behavioral.velocity_3_1h);
    print_rule("5 txns/24h", report.behavioral.velocity_5_24h);
    print_rule("amount |z|>3", report.behavioral.amount_z_gt_3);
    print_rule("same merchant <=10m", report.behavioral.same_merchant_10m);
    println!("  graph fan-out/clustering = n/a (T4)");

    println!("leakage (single-feature AUC vs fraud_target)");
    for (name, value) in &report.leakage.features {
        match value {
            Some(auc) => println!("  {name:<20} {auc:.4}"),
            None => println!("  {name:<20} n/a"),
        }
    }
    println!("  correlation drift = n/a (T7)");
}

fn print_rule(name: &str, rates: ClassRates) {
    println!(
        "  {name:<20} fraud {:>7.2}% | legit {:>7.2}%",
        rates.fraud, rates.legit
    );
}

fn read_parquet_df(path: &Path) -> Result<DataFrame, Box<dyn Error>> {
    let file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    Ok(ParquetReader::new(file).finish()?)
}

fn target_map(df: &DataFrame) -> Result<HashMap<String, bool>, Box<dyn Error>> {
    let ids = string_values(df, "transaction_id")?;
    let targets = bool_values(df, "fraud_target")?;
    if ids.len() != targets.len() {
        return Err("fraud_metadata.parquet has mismatched column lengths".into());
    }
    let mut map = HashMap::with_capacity(ids.len());
    for (id, target) in ids.into_iter().zip(targets) {
        if let (Some(id), Some(target)) = (id, target) {
            map.insert(id, target);
        }
    }
    Ok(map)
}

fn string_values(df: &DataFrame, name: &str) -> Result<Vec<Option<String>>, PolarsError> {
    Ok(df
        .column(name)?
        .str()?
        .into_iter()
        .map(|value| value.map(String::from))
        .collect())
}

fn bool_values(df: &DataFrame, name: &str) -> Result<Vec<Option<bool>>, PolarsError> {
    Ok(df.column(name)?.bool()?.into_iter().collect())
}

fn f64_values(df: &DataFrame, name: &str) -> Result<Vec<Option<f64>>, PolarsError> {
    Ok(df.column(name)?.f64()?.into_iter().collect())
}

fn timestamp_values(df: &DataFrame, name: &str) -> Result<Vec<Option<i64>>, PolarsError> {
    let column = df.column(name)?;
    if let Ok(datetime) = column.datetime() {
        Ok(datetime
            .as_datetime_iter()
            .map(|value| value.map(|dt| dt.and_utc().timestamp()))
            .collect())
    } else {
        Ok(column
            .str()?
            .into_iter()
            .map(|value| value.and_then(parse_ts))
            .collect())
    }
}

fn parse_ts(raw: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.timestamp())
}

fn hour_of_day(ts: i64) -> f64 {
    DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.hour() as f64)
        .unwrap_or(0.0)
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        f64::NAN
    } else {
        100.0 * n as f64 / total as f64
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        f64::NAN
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn skewness(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    if n < 3.0 {
        return f64::NAN;
    }
    let mean = values.iter().sum::<f64>() / n;
    let m2 = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let m3 = values.iter().map(|v| (v - mean).powi(3)).sum::<f64>() / n;
    if m2 == 0.0 {
        0.0
    } else {
        m3 / m2.powf(1.5)
    }
}

fn lag1_autocorr(values: &[f64]) -> Option<f64> {
    if values.len() < 3 {
        return None;
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    if var == 0.0 {
        return None;
    }
    let cov: f64 = values
        .windows(2)
        .map(|w| (w[0] - mean) * (w[1] - mean))
        .sum();
    Some(cov / var)
}

fn fano(times: &[i64], min_ts: i64, buckets: usize) -> Option<f64> {
    if times.is_empty() || buckets == 0 {
        return None;
    }
    let mut counts = vec![0usize; buckets];
    for &ts in times {
        let idx = ((ts - min_ts) / 3600) as usize;
        if idx < buckets {
            counts[idx] += 1;
        }
    }
    let mean = counts.iter().sum::<usize>() as f64 / buckets as f64;
    if mean == 0.0 {
        return None;
    }
    let variance = counts
        .iter()
        .map(|&c| {
            let d = c as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / buckets as f64;
    Some(variance / mean)
}

/// Mark transactions that sit inside a `window`-second window with at least
/// `need` transactions.
fn burst_members(times: &[i64], window: i64, need: usize) -> Vec<bool> {
    let mut member = vec![false; times.len()];
    if times.len() < need {
        return member;
    }
    let mut start = 0;
    for end in 0..times.len() {
        while times[end] - times[start] > window {
            start += 1;
        }
        if end - start + 1 >= need {
            for mark in member.iter_mut().take(end + 1).skip(start) {
                *mark = true;
            }
        }
    }
    member
}

fn same_merchant_hits(samples: &[&Sample], times: &[i64]) -> Vec<bool> {
    let mut hit = vec![false; samples.len()];
    for i in 0..samples.len() {
        for j in (i + 1)..samples.len() {
            if times[j] - times[i] > 600 {
                break;
            }
            if samples[i].merchant_id == samples[j].merchant_id {
                hit[i] = true;
                hit[j] = true;
            }
        }
    }
    hit
}

fn amount_z_hits(samples: &[&Sample]) -> Vec<bool> {
    let amounts: Vec<f64> = samples.iter().map(|s| s.amount).collect();
    let n = amounts.len() as f64;
    if n == 0.0 {
        return Vec::new();
    }
    let mean = amounts.iter().sum::<f64>() / n;
    let std = (amounts.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / n).sqrt();
    if std == 0.0 {
        return vec![false; amounts.len()];
    }
    amounts
        .iter()
        .map(|a| ((a - mean) / std).abs() > 3.0)
        .collect()
}

fn auc_feature<F>(samples: &[Sample], score: F) -> Option<f64>
where
    F: Fn(&Sample) -> f64,
{
    let scores: Vec<(f64, bool)> = samples.iter().map(|s| (score(s), s.fraud_target)).collect();
    auc(&scores)
}

fn categorical_auc<F>(samples: &[Sample], value: F) -> Option<f64>
where
    F: for<'a> Fn(&'a Sample) -> &'a str,
{
    let categories: BTreeSet<&str> = samples.iter().map(&value).collect();
    let mut best: Option<f64> = None;
    for category in categories {
        let scores: Vec<(f64, bool)> = samples
            .iter()
            .map(|s| (if value(s) == category { 1.0 } else { 0.0 }, s.fraud_target))
            .collect();
        if let Some(auc) = auc(&scores) {
            let oriented = auc.max(1.0 - auc);
            best = Some(best.map_or(oriented, |b| b.max(oriented)));
        }
    }
    best
}

/// Rank-based AUC with average ranks for ties.
fn auc(scores: &[(f64, bool)]) -> Option<f64> {
    let positives = scores.iter().filter(|(_, target)| *target).count();
    let negatives = scores.len() - positives;
    if positives == 0 || negatives == 0 {
        return None;
    }

    let mut sorted = scores.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut ranks = vec![0.0; sorted.len()];
    let mut i = 0;
    while i < sorted.len() {
        let mut j = i;
        while j + 1 < sorted.len() && sorted[j + 1].0 == sorted[i].0 {
            j += 1;
        }
        let average_rank = ((i + 1) as f64 + (j + 1) as f64) / 2.0;
        for rank in ranks.iter_mut().take(j + 1).skip(i) {
            *rank = average_rank;
        }
        i = j + 1;
    }

    let sum_positive: f64 = sorted
        .iter()
        .zip(&ranks)
        .filter(|(score, _)| score.1)
        .map(|(_, rank)| *rank)
        .sum();
    let p = positives as f64;
    Some((sum_positive - p * (p + 1.0) / 2.0) / (p * negatives as f64))
}
