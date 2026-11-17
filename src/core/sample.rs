use rand::Rng;
use serde::{Deserialize, Serialize};

/// An inclusive `[min, max]` range a count is sampled from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RangeControl {
    pub min: usize,
    pub max: usize,
}

impl RangeControl {
    /// Sample a count uniformly in `[min, max]`, both inclusive. A `max` below
    /// `min` is treated as a fixed `min`.
    pub fn sample(&self, rng: &mut impl Rng) -> usize {
        rng.random_range(self.min..=self.max.max(self.min))
    }
}

/// Select the index of the first weight whose value exceeds a running total,
/// starting from an already-drawn uniform value `r`.
///
/// This is the shared "draw once, then walk the weights subtracting as you go"
/// loop used across the generators. The caller performs the draw itself so the
/// exact sampling primitive (`random` vs `random_range(0.0..total)`) is preserved
/// per call site, and so no weights need to be allocated. Iteration order is
/// significant — pass weights in the caller's iteration order.
///
/// Returns `None` when no weight matches (a floating-point edge); callers apply
/// their own fallback.
pub fn weighted_index(mut r: f64, weights: &[f64]) -> Option<usize> {
    for (i, w) in weights.iter().enumerate() {
        if r < *w {
            return Some(i);
        }
        r -= w;
    }
    None
}
