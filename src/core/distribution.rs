use rand::Rng;

/// Draw an exponential inter-arrival time with the given mean.
///
/// Deterministic for a given RNG stream; avoids pulling in `rand_distr` for a
/// single distribution.
pub fn exponential(mean: f64, rng: &mut impl Rng) -> f64 {
    let u: f64 = rng.random::<f64>().max(f64::MIN_POSITIVE);
    -mean * u.ln()
}
