use crate::config::{expand_env_vars, Config};
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
api:
  enabled: true
logging:
  level: "debug"
discovery:
  enabled: true
  interval: 600
health:
  interval: 30
snmp:
  enabled: false
  interval: 120
scheduler:
  concurrency: 5
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
alert:
  enabled: true
  interval: 60
  rules: []
synthetic_checks:
  enabled: true
  interval: 60
  checks: []
telegram:
  enabled: false
  bot_token: ""
  chat_id: ""
  parse_mode: "HTML"
  timeout_seconds: 10
  max_retries: 3
  retry_backoff_seconds: 5
auth:
  enabled: true
  tokens: {}
  public_paths:
    - "/health"
    - "/metrics"
  token_header: "Authorization"
tls:
  enabled: false
ssrf:
  enabled: true
rate_limit:
  enabled: true
"#;

    fs::write(&config_path, yaml_content).unwrap();

    let config = Config::from_path(&config_path).unwrap();

    assert_eq!(config.agent.name, "test-agent");
    assert_eq!(config.agent.listen, "127.0.0.1:9090");
    assert_eq!(config.storage.path, "test.sqlite");
    assert!(config.metrics.enabled);
    assert!(config.api.enabled);
    assert_eq!(config.logging.level, "debug");
    assert!(config.discovery.enabled);
    assert_eq!(config.discovery.interval, 600);
    assert_eq!(config.health.interval, 30);
    assert!(!config.snmp.enabled);
    assert_eq!(config.snmp.interval, 120);
    assert_eq!(config.scheduler.concurrency, 5);
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
    // yaml_serde will fail if required fields are missing
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Failed to parse YAML") || err.contains("missing field"));
}

#[test]
fn test_expand_env_vars_simple() {
    std::env::set_var("TEST_VAR", "test_value");
    let input = "value: ${TEST_VAR}";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, "value: test_value");
    std::env::remove_var("TEST_VAR");
}

#[test]
fn test_expand_env_vars_with_default() {
    let input = "value: ${MISSING_VAR:-default_value}";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, "value: default_value");
}

#[test]
fn test_expand_env_vars_missing_required() {
    let input = "value: ${MISSING_REQUIRED_VAR}";
    let result = expand_env_vars(input);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Required environment variable"));
}

#[test]
fn test_expand_env_vars_multiple() {
    std::env::set_var("VAR1", "value1");
    std::env::set_var("VAR2", "value2");
    let input = "v1: ${VAR1}, v2: ${VAR2}";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, "v1: value1, v2: value2");
    std::env::remove_var("VAR1");
    std::env::remove_var("VAR2");
}

#[test]
fn test_expand_env_vars_no_vars() {
    let input = "plain text without variables";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, "plain text without variables");
}

#[test]
fn test_expand_env_vars_empty_default() {
    let input = "value: ${MISSING_VAR:-}";
    let result = expand_env_vars(input).unwrap();
    assert_eq!(result, "value: ");
}
