use serde::{Deserialize, Serialize};

/// Ground-truth fraud labels for injected transactions.
///
/// `fraud_target` is truth (was fraud actually injected); `is_fraud` on the
/// transaction is the noisy operational label. Keeping both is what makes the
/// dataset usable for studying label noise and leakage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudMetadata {
    pub transaction_id: String,
    pub fraud_target: bool,
    pub fraud_type: String,
    pub label_noise: String,
    pub injector_version: String,
    pub geo_anomaly: bool,
    pub device_anomaly: bool,
    pub ip_anomaly: bool,
    pub flags: Option<Vec<String>>,
    pub burst_session: bool,
    pub burst_seq: Option<i32>,
    pub campaign_id: Option<String>,
    pub campaign_type: Option<String>,
    pub campaign_phase: Option<String>,
    pub campaign_day_number: Option<i32>,
}
