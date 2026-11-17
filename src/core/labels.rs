use rand::rngs::StdRng;
use rand::Rng;

/// Decide whether an event is a fraud target and apply label noise.
///
/// `effective_share` should already factor in any customer-level multiplier.
/// Returns `(fraud_target, is_fraud_label, label_noise)`, where `label_noise` is
/// one of `"none"`, `"fn"` (a true fraud labelled benign), or `"fp"` (a benign
/// event labelled fraud).
pub fn determine_targeting(
    effective_share: f64,
    fn_rate: f64,
    fp_rate: f64,
    rng: &mut StdRng,
) -> (bool, bool, String) {
    let fraud_target = rng.random::<f64>() < effective_share;

    let mut is_fraud_label = fraud_target;
    let mut label_noise = "none".to_string();

    if fraud_target && rng.random::<f64>() < fn_rate {
        is_fraud_label = false;
        label_noise = "fn".to_string();
    } else if !fraud_target && rng.random::<f64>() < fp_rate {
        is_fraud_label = true;
        label_noise = "fp".to_string();
    }

    (fraud_target, is_fraud_label, label_noise)
}
