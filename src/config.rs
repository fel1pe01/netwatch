use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        }
    }
}

impl Config {
    pub fn from_path(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Could not read config at {}", path.display()))?;
        let mut cfg: Config = serde_yaml::from_str(&content)
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
