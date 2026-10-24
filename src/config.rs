use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use yaml_serde;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub listen: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageConfig {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MetricsConfig {
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiConfig {
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HealthConfig {
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SnmpConfig {
    pub enabled: bool,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TelegramConfig {
    pub enabled: bool,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default = "default_parse_mode")]
    pub parse_mode: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_seconds: u64,
}

fn default_parse_mode() -> String {
    "HTML".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_backoff() -> u64 {
    5
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyntheticChecksConfig {
    pub enabled: Option<bool>,
    pub interval: Option<u64>,
    pub checks: Vec<SyntheticCheck>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyntheticCheck {
    pub id: String,
    pub name: String,
    pub check_type: SyntheticCheckType,
    pub target: String,
    pub port: Option<u16>,
    pub expected_status: Option<u16>,
    pub expected_body: Option<String>,
    pub headers: Option<Vec<(String, String)>>,
    pub timeout_ms: Option<u64>,
    pub record_type: Option<String>,
    pub check_expiry_days: Option<u64>,
    pub verify_hostname: Option<bool>,
    pub enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SyntheticCheckType {
    Http,
    Https,
    Dns,
    Tcp,
    Tls,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub concurrency: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CapacityConfig {
    pub enabled: Option<bool>,
    pub interval: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AnomalyConfig {
    pub enabled: Option<bool>,
    pub interval: Option<u64>,
    pub min_samples: Option<usize>,
    pub lookback_window_seconds: Option<i64>,
    pub sensitivity: Option<f64>,
    pub min_z_score: Option<f64>,
    pub enable_spike_detection: Option<bool>,
    pub enable_drift_detection: Option<bool>,
    pub enable_pattern_break_detection: Option<bool>,
    pub anomaly_cooldown_seconds: Option<i64>,
    pub min_absolute_change: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlertConfig {
    pub enabled: Option<bool>,
    pub interval: Option<u64>,
    pub rules: Vec<AlertRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub entity_type: String,
    pub protocol: Option<String>,
    pub condition: AlertCondition,
    pub severity: AlertSeverity,
    pub cooldown_seconds: u64,
    pub labels: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertCondition {
    Down,
    Up,
    LatencyThreshold {
        threshold_ms: f64,
    },
    LatencyPercentile {
        percentile: f64,
        threshold_ms: f64,
        window_seconds: i64,
    },
    FailureRate {
        threshold: f64,
        window_seconds: i64,
    },
    SustainedDegradation {
        failure_count: u32,
        window_seconds: i64,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "info"),
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            enabled: Some(true),
            interval: Some(300),
            min_samples: Some(10),
            lookback_window_seconds: Some(3600),
            sensitivity: Some(1.0),
            min_z_score: Some(3.0),
            enable_spike_detection: Some(true),
            enable_drift_detection: Some(true),
            enable_pattern_break_detection: Some(false),
            anomaly_cooldown_seconds: Some(300),
            min_absolute_change: Some(10.0),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub agent: AgentConfig,
    pub storage: StorageConfig,
    pub metrics: MetricsConfig,
    pub api: ApiConfig,
    pub logging: LoggingConfig,
    pub discovery: DiscoveryConfig,
    pub health: HealthConfig,
    pub snmp: SnmpConfig,
    pub scheduler: SchedulerConfig,
    pub capacity: CapacityConfig,
    pub anomaly: AnomalyConfig,
    pub alert: AlertConfig,
    pub synthetic_checks: SyntheticChecksConfig,
    pub telegram: TelegramConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            agent: AgentConfig {
                name: "netwatch".to_string(),
                listen: "0.0.0.0:8080".to_string(),
            },
            storage: StorageConfig {
                path: "netwatch.sqlite".to_string(),
            },
            metrics: MetricsConfig { enabled: true },
            api: ApiConfig { enabled: true },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            discovery: DiscoveryConfig {
                enabled: true,
                interval: 300,
            },
            health: HealthConfig { interval: 15 },
            snmp: SnmpConfig {
                enabled: true,
                interval: 60,
            },
            scheduler: SchedulerConfig { concurrency: 10 },
            capacity: CapacityConfig {
                enabled: Some(true),
                interval: Some(3600),
            },
            anomaly: AnomalyConfig::default(),
            alert: AlertConfig {
                enabled: Some(true),
                interval: Some(60),
                rules: vec![],
            },
            synthetic_checks: SyntheticChecksConfig {
                enabled: Some(true),
                interval: Some(60),
                checks: vec![],
            },
            telegram: TelegramConfig {
                enabled: false,
                bot_token: String::new(),
                chat_id: String::new(),
                parse_mode: "HTML".to_string(),
                timeout_seconds: 10,
                max_retries: 3,
                retry_backoff_seconds: 5,
            },
        }
    }
}

impl Config {
    pub fn from_path(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Could not read config at {}", path.display()))?;
        let mut cfg: Config = yaml_serde::from_str(&content)
            .with_context(|| format!("Failed to parse YAML at {}", path.display()))?;

        // Override with environment variables if present
        if let Ok(path) = std::env::var("NETWATCH_STORAGE_PATH") {
            cfg.storage.path = path;
        }
        if let Ok(level) = std::env::var("NETWATCH_LOG_LEVEL") {
            cfg.logging.level = level;
        }
        if let Ok(listen) = std::env::var("NETWATCH_LISTEN") {
            cfg.agent.listen = listen;
        }

        Ok(cfg)
    }
}
