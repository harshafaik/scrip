use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub use crate::core::sample::RangeControl;

/// The complete generator configuration. Serialisable to/from the YAML files in
/// `config/`. The embedded defaults make `scrip` runnable with no config on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub rules: FraudRules,
    pub tuning: FraudTuning,
    pub customer: CustomerConfig,
    pub transactions: TransactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudRules {
    pub global: GlobalConfig,
    pub payment_channels: BTreeMap<String, ChannelConfig>,
    pub fraud_patterns: BTreeMap<String, Vec<f64>>,
    pub device_patterns: DevicePatterns,
    pub fraud_injector: FraudInjectorConfig,
    pub fraud_campaigns: CampaignsConfig,
    pub temporal_patterns: BTreeMap<String, TemporalPatternConfig>,
    pub failure_reasons_by_type: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub fraud_flags: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub seed: i32,
    pub base_currency: String,
    pub default_country: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    pub market_share: f64,
    pub risk_level: f64,
    pub user_agents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePatterns {
    pub ip_prefixes: Vec<String>,
    pub bot_user_agent_prefix: String,
    pub known_bad_prefixes: BTreeMap<String, f64>,
    #[serde(default)]
    pub ato_ua_pool: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudInjectorConfig {
    pub target_share: f64,
    pub default_fp_rate: f64,
    pub default_fn_rate: f64,
    pub customer_fraud_multiplier: f64,
    pub profiles: BTreeMap<String, FraudProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudProfileConfig {
    pub frequency: f64,
    pub amount_pattern: String,
    pub amount_strategy: Option<String>,
    pub amount_multiplier: Option<String>,
    pub channel_bias: BTreeMap<String, f64>,
    pub geo_anomaly_prob: f64,
    pub temporal_anomaly_prob: Option<f64>,
    pub merchant_bias: Option<BTreeMap<String, f64>>,
    #[serde(default)]
    pub behavioral_flags: Option<BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignsConfig {
    pub target_campaign_share: f64,
    pub profiles: BTreeMap<String, FraudCampaignConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudCampaignConfig {
    pub frequency: f64,
    pub amount_escalation: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPatternConfig {
    pub hourly_weights: Vec<f64>,
    pub daily_weights: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudTuning {
    pub probabilities: ProbabilitiesConfig,
    pub defaults: DefaultsConfig,
    pub campaigns: TuningCampaignConfig,
    pub salts: SaltsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbabilitiesConfig {
    pub geo_anomaly: f64,
    pub device_anomaly: f64,
    pub ip_anomaly: f64,
    pub failure: f64,
    pub chargeback: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub geo_anomaly_country: String,
    pub fallback_failure_reason: String,
    pub chargeback_days: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuningCampaignConfig {
    pub ato_escalation_rate: f64,
    pub coordinated_scam_ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaltsConfig {
    pub injector: i32,
    pub mutator: i32,
    pub campaign: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerConfig {
    pub names: NamesConfig,
    pub email: EmailConfig,
    pub locations: LocationsConfig,
    pub financials: FinancialsConfig,
    pub registration: RegistrationConfig,
    pub control: GenerationControl,
    pub device_profiles: DeviceProfilesConfig,
    pub isp_assignment: ISPAssignmentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProfilesConfig {
    pub location_shares: BTreeMap<String, LocationSharesConfig>,
    pub android_ua_pool: Vec<String>,
    pub ios_ua_pool: Vec<String>,
    pub upi_app_ua_pool: Vec<String>,
    pub desktop_ua_pool: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationSharesConfig {
    pub android_share: f64,
    pub ios_share: f64,
    pub upi_app_share: f64,
    pub desktop_share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ISPAssignmentConfig {
    pub shares: BTreeMap<String, BTreeMap<String, f64>>,
    pub subnets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamesConfig {
    pub first_names: Vec<String>,
    pub last_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationsConfig {
    pub types: Vec<String>,
    pub metro_cities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinancialsConfig {
    pub base_spend: BTreeMap<String, f64>,
    pub credit_score: CreditScoreConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditScoreConfig {
    pub base: i32,
    pub age_weight: f64,
    pub min: u16,
    pub max: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationConfig {
    pub lookback_years: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationControl {
    pub customer_count: usize,
    #[serde(default = "default_accounts_per_customer")]
    pub accounts_per_customer: RangeControl,
    #[serde(default = "default_cards_per_account")]
    pub cards_per_account: RangeControl,
    pub transactions_per_customer: RangeControl,
    pub parallelism: ParallelismControl,
}

/// RiskFabric's default: one account, with a second for half of customers.
fn default_accounts_per_customer() -> RangeControl {
    RangeControl { min: 1, max: 2 }
}

/// RiskFabric issues exactly one card per account.
fn default_cards_per_account() -> RangeControl {
    RangeControl { min: 1, max: 1 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelismControl {
    pub customer_gen_threads: usize,
    pub transaction_gen_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    pub transactions: TransactionBaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionBaseConfig {
    pub merchant_categories: Vec<CategoryMccMap>,
    pub amount_range: Vec<f64>,
    pub success_rate: f64,
    pub card_present_probability: f64,
    pub geo_bounds: GeoBounds,
    pub lookback_days: i32,
    pub status_codes: BTreeMap<String, Vec<String>>,
    pub temporal_patterns: TemporalPatternConfig,
    #[serde(default)]
    pub sessions: SessionsConfig,
    #[serde(default)]
    pub streaming_mode: bool,
    #[serde(default = "default_streaming_rate")]
    pub streaming_rate: u32,
}

fn default_streaming_rate() -> u32 {
    100
}

/// Session-based arrival model and the acceptance bands for its metrics.
///
/// Bands were derived from `t1_probe.py` with the default parameters below
/// (sizes 70/25/4/1, 300s within-session mean, hour-weighted starts on uniform
/// days). This generator also applies day weights. Re-run the probe and update
/// the bands whenever these parameters change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsConfig {
    /// Probability weights for session sizes 1..=N.
    pub size_weights: Vec<f64>,
    /// Mean within-session inter-transaction time, in seconds.
    pub within_iet_mean_seconds: f64,
    /// Acceptance band for the hourly-count Fano factor.
    pub fano_min: f64,
    /// Acceptance bound for |lag-1 IET autocorrelation|.
    pub iet_autocorr_bound: f64,
    /// Acceptance band for transaction-level burst membership on a sparse fixture.
    pub burst_membership_min: f64,
    pub burst_membership_max: f64,
    /// Bimodality acceptance bounds.
    pub within_session_p50_min_seconds: f64,
    pub within_session_p50_max_seconds: f64,
    pub inter_session_p50_min_seconds: f64,
    pub iet_p90_p10_min_ratio: f64,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            size_weights: vec![0.70, 0.25, 0.04, 0.01],
            within_iet_mean_seconds: 300.0,
            fano_min: 1.3,
            iet_autocorr_bound: 0.10,
            burst_membership_min: 0.05,
            burst_membership_max: 0.25,
            within_session_p50_min_seconds: 10.0,
            within_session_p50_max_seconds: 600.0,
            inter_session_p50_min_seconds: 7200.0,
            iet_p90_p10_min_ratio: 20.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryMccMap {
    pub name: String,
    pub mcc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoBounds {
    pub lat_range: Vec<f64>,
    pub long_range: Vec<f64>,
}

const DEFAULT_CUSTOMER: &str = include_str!("../config/customer_config.yaml");
const DEFAULT_TRANSACTION: &str = include_str!("../config/transaction_config.yaml");
const DEFAULT_RULES: &str = include_str!("../config/fraud_rules.yaml");
const DEFAULT_TUNING: &str = include_str!("../config/fraud_tuning.yaml");

impl AppConfig {
    /// Load every section from a directory of YAML files (`customer_config.yaml`,
    /// `transaction_config.yaml`, `fraud_rules.yaml`, `fraud_tuning.yaml`).
    pub fn from_dir(dir: &Path) -> Self {
        let read = |name: &str| {
            let p = dir.join(name);
            std::fs::read_to_string(&p)
                .unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()))
        };
        Self::from_strs(
            &read("fraud_rules.yaml"),
            &read("fraud_tuning.yaml"),
            &read("customer_config.yaml"),
            &read("transaction_config.yaml"),
        )
    }

    /// The built-in defaults, embedded at compile time. Requires no files on disk.
    pub fn default_embedded() -> Self {
        Self::from_strs(
            DEFAULT_RULES,
            DEFAULT_TUNING,
            DEFAULT_CUSTOMER,
            DEFAULT_TRANSACTION,
        )
    }

    fn from_strs(rules: &str, tuning: &str, customer: &str, transactions: &str) -> Self {
        AppConfig {
            rules: serde_yaml::from_str(rules).expect("parse fraud_rules"),
            tuning: serde_yaml::from_str(tuning).expect("parse fraud_tuning"),
            customer: serde_yaml::from_str(customer).expect("parse customer_config"),
            transactions: serde_yaml::from_str(transactions).expect("parse transaction_config"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_config_without_sessions_uses_defaults() {
        let marker = "  # Session-based arrival model.";
        assert!(DEFAULT_TRANSACTION.contains(marker));
        let yaml = DEFAULT_TRANSACTION.split(marker).next().unwrap();
        let parsed: TransactionConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(
            parsed.transactions.sessions.size_weights,
            vec![0.70, 0.25, 0.04, 0.01]
        );
        assert_eq!(parsed.transactions.sessions.within_iet_mean_seconds, 300.0);
    }

    #[test]
    fn generation_control_defaults_missing_ranges() {
        let control: GenerationControl = serde_yaml::from_str(
            "customer_count: 10\n\
             transactions_per_customer: {min: 1, max: 2}\n\
             parallelism: {customer_gen_threads: 1, transaction_gen_threads: 1}\n",
        )
        .unwrap();

        assert_eq!(control.accounts_per_customer.min, 1);
        assert_eq!(control.accounts_per_customer.max, 2);
        assert_eq!(control.cards_per_account.min, 1);
        assert_eq!(control.cards_per_account.max, 1);
    }
}
