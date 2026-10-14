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
    pub path: String,
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
pub struct PortsConfig {
    pub enabled: bool,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SnmpConfig {
    pub enabled: bool,
    pub interval: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub concurrency: usize,
    pub rate_limit: u64,
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
    pub ports: PortsConfig,
    pub snmp: SnmpConfig,
    pub scheduler: SchedulerConfig,
    pub capacity: CapacityConfig,
    pub anomaly: AnomalyConfig,
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
            metrics: MetricsConfig {
                enabled: true,
                path: "metrics.sqlite".to_string(),
            },
            api: ApiConfig { enabled: true },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            discovery: DiscoveryConfig {
                enabled: true,
                interval: 300,
            },
            health: HealthConfig { interval: 15 },
            ports: PortsConfig {
                enabled: true,
                interval: 900,
            },
            snmp: SnmpConfig {
                enabled: true,
                interval: 60,
            },
            scheduler: SchedulerConfig {
                concurrency: 10,
                rate_limit: 1000,
            },
            capacity: CapacityConfig {
                enabled: Some(true),
                interval: Some(3600),
            },
            anomaly: AnomalyConfig::default(),
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
