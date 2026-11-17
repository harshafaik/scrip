//! Arrival-time processes.
//!
//! The generator consumes session starts through [`ArrivalProcess`], so a
//! different process (for example a self-exciting one) can be swapped in
//! without touching the transaction generator.

use crate::core::sample::weighted_index;
use rand::rngs::StdRng;
use rand::Rng;

pub trait ArrivalProcess {
    /// Return `count` arrival offsets, in seconds from the start of a
    /// `window_seconds`-long window.
    fn sample_starts(&self, count: usize, window_seconds: i64, rng: &mut StdRng) -> Vec<i64>;
}

/// Independent arrivals weighted by time of day and day of week.
pub struct WeightedArrival {
    pub hourly_weights: Vec<f64>,
    pub daily_weights: Vec<f64>,
}

impl ArrivalProcess for WeightedArrival {
    fn sample_starts(&self, count: usize, window_seconds: i64, rng: &mut StdRng) -> Vec<i64> {
        let window_seconds = window_seconds.max(1);
        let days = (window_seconds / 86_400).max(1);
        let hourly = padded(&self.hourly_weights, 24);
        let daily = padded(&self.daily_weights, 7);
        let hourly_total: f64 = hourly.iter().sum();
        let max_daily = daily.iter().copied().fold(0.0, f64::max);

        let mut starts = Vec::with_capacity(count);
        for _ in 0..count {
            let day = loop {
                let candidate = rng.random_range(0..days);
                let weight = daily[(candidate % 7) as usize];
                if max_daily <= 0.0 || rng.random::<f64>() * max_daily < weight {
                    break candidate;
                }
            };

            let hour = if hourly_total > 0.0 {
                weighted_index(rng.random::<f64>() * hourly_total, &hourly).unwrap_or(0)
            } else {
                rng.random_range(0..24)
            };

            let second = rng.random_range(0..3_600);
            let offset = day * 86_400 + (hour as i64) * 3_600 + second;
            starts.push(offset.min(window_seconds - 1));
        }
        starts
    }
}

fn padded(weights: &[f64], len: usize) -> Vec<f64> {
    let mut out = vec![0.0; len];
    for (slot, weight) in out.iter_mut().zip(weights.iter()) {
        if *weight > 0.0 {
            *slot = *weight;
        }
    }
    if out.iter().all(|weight| *weight <= 0.0) {
        out.fill(1.0);
    }
    out
}
