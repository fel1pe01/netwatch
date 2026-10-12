use crate::config::Config;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_config_from_valid_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("netwatch.yaml");

    let yaml_content = r#"
agent:
  name: "test-agent"
  listen: "127.0.0.1:9090"
storage:
  path: "test.sqlite"
metrics:
  enabled: true
  path: "metrics.sqlite"
api:
  enabled: true
logging:
  level: "debug"
discovery:
  enabled: true
  interval: 600
health:
  interval: 30
ports:
  enabled: false
  interval: 1800
snmp:
  enabled: false
  interval: 120
scheduler:
  concurrency: 5
  rate_limit: 500
capacity:
  enabled: true
  interval: 3600
anomaly:
  enabled: true
  interval: 300
  min_samples: 10
  lookback_window_seconds: 3600
  sensitivity: 1.0
  min_z_score: 3.0
  enable_spike_detection: true
  enable_drift_detection: true
  enable_pattern_break_detection: false
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let config = Config::from_path(&config_path).unwrap();

    assert_eq!(config.agent.name, "test-agent");
    assert_eq!(config.agent.listen, "127.0.0.1:9090");
    assert_eq!(config.storage.path, "test.sqlite");
    assert!(config.metrics.enabled);
    assert_eq!(config.metrics.path, "metrics.sqlite");
    assert!(config.api.enabled);
    assert_eq!(config.logging.level, "debug");
    assert!(config.discovery.enabled);
    assert_eq!(config.discovery.interval, 600);
    assert_eq!(config.health.interval, 30);
    assert!(!config.ports.enabled);
    assert_eq!(config.ports.interval, 1800);
    assert!(!config.snmp.enabled);
    assert_eq!(config.snmp.interval, 120);
    assert_eq!(config.scheduler.concurrency, 5);
    assert_eq!(config.scheduler.rate_limit, 500);
}

#[test]
fn test_config_from_missing_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("nonexistent.yaml");

    let result = Config::from_path(&config_path);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Could not read config") || err.contains("No such file"));
}

#[test]
fn test_config_from_invalid_yaml() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("invalid.yaml");

    let invalid_yaml = "agent: { name: }"; // Invalid YAML - missing value
    fs::write(&config_path, invalid_yaml).unwrap();

    let result = Config::from_path(&config_path);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Failed to parse YAML") || err.contains("parse"));
}

#[test]
fn test_config_from_empty_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("empty.yaml");

    fs::write(&config_path, "").unwrap();

    let result = Config::from_path(&config_path);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Failed to parse YAML") || err.contains("parse"));
}

#[test]
fn test_config_from_partial_file() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("partial.yaml");

    // Only provide agent config, let others use defaults
    let yaml_content = r#"
agent:
  name: "partial-agent"
  listen: "0.0.0.0:7777"
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let result = Config::from_path(&config_path);
    // serde_yaml will fail if required fields are missing
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Failed to parse YAML") || err.contains("missing field"));
}
