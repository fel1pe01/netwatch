# Netwatch Testing Guide

This guide covers the testing framework, how to run tests, and how to write new tests.

## Test Overview

Netwatch uses a multi-layered testing approach:

| Layer | Location | Count | Purpose |
|-------|----------|-------|---------|
| Unit tests | `src/*_test.rs`, inline `#[cfg(test)]` | ~20 | Module-level logic |
| Integration tests | `tests/*.rs` | ~70 | API, database, workflows |
| E2E smoke tests | `scripts/e2e_smoke.sh` | 40 checks | Full system validation |

**Total: ~90 automated tests + 40 E2E checks**

## Running Tests

### Unit & Integration Tests

```bash
# Run all tests
cargo test --locked --all-targets --all-features --no-fail-fast

# Run specific test file
cargo test --test http_integration_tests

# Run specific test
cargo test test_http_health

# Run with output
cargo test -- --nocapture

# Run with threads
cargo test -- --test-threads=4
```

### Test Categories

```bash
# Unit tests only (fast)
cargo test --lib

# Integration tests only
cargo test --tests

# Specific test file
cargo test --test metrics_cardinality_test

# Doc tests
cargo test --doc
```

### E2E Smoke Tests

```bash
# Requires release binary
cargo build --locked --release
./scripts/e2e_smoke.sh

# Keep logs on failure
E2E_KEEP_LOGS=1 ./scripts/e2e_smoke.sh

# Custom binary
NETWATCH_BIN=/path/to/netwatch ./scripts/e2e_smoke.sh
```

### Pre-push Validation

```bash
# Full validation (matches CI)
./scripts/check.sh

# Quick validation (no docker, no audit)
./scripts/check.sh --quick --skip-docker --skip-audit

# Skip E2E
./scripts/check.sh --skip-e2e
```

## Test Structure

### Unit Tests (Inline)

Located in source files with `#[cfg(test)]`:

```rust
// src/metrics.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_metrics_creation() {
        let metrics = AppMetrics::new();
        assert!(metrics.info.get() > 0);
    }
}
```

Run: `cargo test --lib`

### Integration Tests (`tests/`)

Each file is a separate crate:

```
tests/
├── integration_test.rs       # Core workflow tests
├── cli_tests.rs              # CLI command tests
├── discovery_tests.rs        # Network discovery
├── monitoring_tests.rs       # Health checks
├── http_integration_tests.rs # HTTP API tests
├── metrics_cardinality_test.rs # Metric label validation
├── config_file_test.rs       # Config parsing
├── drift_tests.rs            # Drift detection
├── anomaly_tests.rs          # Anomaly detection
├── capacity_tests.rs         # Capacity planning
├── incident_tests.rs         # Incident management
└── root_cause_tests.rs       # Root cause analysis
```

Run: `cargo test --tests`

### E2E Smoke Tests (`scripts/e2e_smoke.sh`)

Bash script that:
1. Starts netwatch server on random port
2. Runs 40 checks against live server
3. Verifies CLI, HTTP API, and metrics
4. Cleans up on exit

**Checks:**
- CLI: version, help, ping, tcp, discover
- HTTP: health, interfaces, routes, monitor/ping, monitor/tcp, placeholders, devices
- Metrics: before/after checks, cardinality, labels

## Writing Tests

### Unit Test Template

```rust
// In src/your_module.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_your_function() {
        // Arrange
        let input = create_test_input();
        
        // Act
        let result = your_function(input);
        
        // Assert
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), expected);
    }
}
```

### Integration Test Template

```rust
// tests/your_feature_test.rs
use netwatch::your_module::YourStruct;
use tempfile::tempdir;
use tokio;

#[tokio::test]
async fn test_your_feature() {
    // Setup
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    
    // Test
    let result = YourStruct::new(&db_path).await;
    
    // Assert
    assert!(result.is_ok());
}
```

### HTTP Test Template

```rust
// tests/http_integration_tests.rs
#[tokio::test]
async fn test_your_endpoint() {
    // Start test server
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();
    
    // Make request
    let resp = client
        .get(format!("{}/api/v1/your-endpoint", base_url))
        .send()
        .await
        .unwrap();
    
    // Assert
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["field"], "expected");
}
```

### Test Utilities

Common helpers in `tests/http_integration_tests.rs`:

```rust
// Spawn isolated test server
async fn spawn_test_server() -> (String, tokio::task::JoinHandle<()>) { ... }

// HTTP POST helper
fn http_post(url: &str, payload: &str) -> (String, String) { ... }

// Assertions
fn assert_status(url: &str, expected: &str, desc: &str) { ... }
fn assert_json_field(url: &str, field: &str, expected: &str, desc: &str) { ... }
fn assert_metrics_contains(pattern: &str, desc: &str) { ... }
```

## Test Database

Tests use isolated temporary databases:

```rust
use tempfile::tempdir;

let dir = tempdir().unwrap();
let db_path = dir.path().join("test.sqlite");
let storage = Storage::connect(db_path.to_str().unwrap()).await?;
```

Migrations run automatically via `sqlx::migrate!("./migrations")`.

## Test Configuration

### Config File Tests

```rust
// tests/config_file_test.rs
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
# ... all required fields
    "#;
    
    fs::write(&config_path, yaml_content).unwrap();
    let config = Config::from_path(&config_path).unwrap();
    
    assert_eq!(config.agent.name, "test-agent");
}
```

### Metrics Cardinality Tests

Ensures metrics only use allowed labels:

```rust
#[test]
fn test_health_check_metrics_only_protocol_label() {
    // Make checks
    HEALTH_CHECK_TOTAL.with_label_values(&["icmp"]).inc();
    
    // Gather metrics
    let registry = prometheus::default_registry();
    let mut buffer = String::new();
    TextEncoder::new().encode_utf8(&registry.gather(), &mut buffer).unwrap();
    
    // Verify only 'protocol' label
    for line in buffer.lines() {
        if line.contains("netwatch_health_checks_total") && line.contains('{') {
            // Extract and validate labels
        }
    }
}
```

## Running Specific Test Suites

```bash
# All tests
cargo test --locked --all-targets --all-features --no-fail-fast

# Unit tests only
cargo test --lib

# Integration tests only
cargo test --tests

# Specific test file
cargo test --test anomaly_tests

# Specific test
cargo test test_spike_detection

# With output
cargo test test_spike_detection -- --nocapture

# Documentation tests
cargo test --doc
```

## Continuous Integration

The project uses GitHub Actions (`.github/workflows/ci.yml`):

```yaml
# Key steps
- cargo fmt --all -- --check
- cargo check --locked --all-targets --all-features
- cargo test --locked --all-targets --all-features --no-fail-fast
- cargo test --doc --all-features
- cargo clippy --locked --all-targets --all-features -- -D warnings
- cargo build --locked --release
- cargo audit
- cargo deny check advisories bans licenses sources
- ./scripts/e2e_smoke.sh
- docker build --pull -t netwatch:check .
```

### Local CI Replica

```bash
# Run all CI checks locally
./scripts/check.sh

# Options
./scripts/check.sh --quick           # fmt, check, test, clippy only
./scripts/check.sh --skip-docker     # Skip docker build
./scripts/check.sh --skip-audit      # Skip cargo audit/deny
./scripts/check.sh --skip-e2e        # Skip e2e smoke test
./scripts/check.sh --fail-fast       # Stop on first failure
```

## Test Data Management

### Temporary Databases

```rust
use tempfile::tempdir;

let dir = tempdir().unwrap();
let db_path = dir.path().join("test.sqlite");
// Automatically cleaned up on drop
```

### Fixtures

```rust
// tests/fixtures.rs
pub fn create_test_device(pool: &SqlitePool) -> String {
    let id = "test-device";
    sqlx::query(
        r#"INSERT INTO devices (id, hostname, ip, ...) VALUES (?, ?, ?, ...)"#
    )
    .bind(id)
    .bind("test-host")
    .bind("192.168.1.1")
    // ...
    .execute(pool)
    .await
    .unwrap();
    id.to_string()
}
```

### Mock Time

```rust
// For time-dependent tests
use chrono::{DateTime, Utc, TimeZone};

let fixed_time = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
// Use in tests instead of Utc::now()
```

## Debugging Tests

### Run with Output

```bash
# Show stdout/stderr
cargo test test_name -- --nocapture

# Show test output on failure
cargo test test_name -- --show-output
```

### Debug Single Test

```bash
# With rust-gdb
cargo test --no-run
rust-gdb target/debug/deps/test_binary-<hash>
```

### Test Logs

```bash
# Enable test logging
RUST_LOG=debug cargo test test_name -- --nocapture

# Specific crate
RUST_LOG=netwatch=debug cargo test test_name
```

## Test Coverage

```bash
# Install cargo-tarpaulin
cargo install cargo-tarpaulin

# Generate coverage
cargo tarpaulin --out Html

# View report
open tarpaulin-report.html
```

## Performance Tests

```bash
# Run benchmarks (if any)
cargo bench

# Profile tests
cargo test --profile release
```

## Test Best Practices

1. **Isolation**: Each test uses its own database
2. **Determinism**: No random values, fixed seeds
3. **Speed**: Unit tests < 100ms, integration < 5s
4. **Clarity**: Descriptive test names (`test_what_when_then`)
5. **Cleanup**: Use `tempfile` for automatic cleanup
6. **Async**: Use `#[tokio::test]` for async tests
7. **No flakiness**: No timing dependencies, no external services

## See Also

- [OPERATIONS.md](OPERATIONS.md) - Operational procedures
- [API.md](API.md) - API reference for HTTP tests
- [CONFIGURATION.md](CONFIGURATION.md) - Test configuration
- [README.md](../README.md) - Quick start
- `./scripts/check.sh --help` - Validation script options