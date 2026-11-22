# Netwatch Configuration Guide

This guide explains all configuration options for Netwatch, how to create configuration files, and how the application loads and merges configuration.

## Configuration Files

Netwatch uses YAML configuration files with the following priority (highest to lowest):

1. **CLI `--config` flag** - Explicit path provided at startup
2. **Environment variables** - Override specific fields
3. **Default values** - Built-in defaults for all options

### File Locations

| Context | Default Path |
|---------|--------------|
| Local development | `./netwatch.yaml` (current directory) |
| Docker container | `/data/netwatch.yaml` (mounted volume) |
| CLI default | `./netwatch.yaml` |

### Example Files

| File | Description |
|------|-------------|
| `config.example.yaml` | Full example with all options documented |
| `test-config.yaml` | Minimal config for testing (discovery disabled) |

## Configuration Structure

The configuration is organized into sections matching the `Config` struct in `src/config.rs`:

```yaml
agent:
  name: "netwatch"
  listen: "127.0.0.1:8080"

storage:
  path: "netwatch.sqlite"
  busy_timeout_ms: 5000

metrics:
  enabled: true

api:
  enabled: true

logging:
  level: "info"

discovery:
  enabled: true
  interval: 300
  absence_multiplier: 3

health:
  interval: 15

snmp:
  enabled: true
  interval: 60

scheduler:
  concurrency: 10
  consecutive_failures_threshold: 3
  consecutive_successes_threshold: 1
  check_results_retention_days: 30
  events_retention_days: 90
  inventory_retention_days: 30
  retention_prune_interval: 3600

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
  anomaly_cooldown_seconds: 300
  min_absolute_change: 10.0

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
  bot_token: "${TELEGRAM_BOT_TOKEN}"
  chat_id: "${TELEGRAM_CHAT_ID}"
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
  cert_file: null
  key_file: null
  min_version: "1.2"
  require_client_auth: false
  ca_file: null

ssrf:
  enabled: true
  allow_private_ips: false
  allow_loopback: false
  allow_link_local: false
  allow_multicast: false
  allow_broadcast: false
  allowed_cidrs: []
  allowed_hostnames: []
  blocked_cidrs: []
  max_redirects: 10
  dns_timeout_ms: 5000
  validate_dns_rebinding: true
  allowed_ports: []
  blocked_ports: []

rate_limit:
  enabled: true
  max_requests: 100
  window_seconds: 60
  key_type: "ip"
  excluded_paths:
    - "/health"
    - "/metrics"
```

## Section Reference

### Agent Configuration

```yaml
agent:
  name: "netwatch"           # Instance identifier
  listen: "127.0.0.1:8080"   # HTTP server bind address (secure by default)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | `"netwatch"` | Instance name for identification |
| `listen` | string | `"127.0.0.1:8080"` | Bind address for HTTP server |

**Environment override**: `NETWATCH_LISTEN`

### Storage Configuration

```yaml
storage:
  path: "netwatch.sqlite"
  busy_timeout_ms: 5000
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | `"netwatch.sqlite"` | SQLite database file path |
| `busy_timeout_ms` | u32 | `5000` | SQLite busy timeout in milliseconds |

**Environment override**: `NETWATCH_STORAGE_PATH`

> **Note**: In Docker, use absolute paths under `/data/` (e.g., `/data/netwatch.sqlite`) which is mounted as a persistent volume.

### Metrics Configuration

```yaml
metrics:
  enabled: true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable Prometheus metrics endpoint at `/metrics` |

### API Configuration

```yaml
api:
  enabled: true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable HTTP API endpoints |

### Logging Configuration

```yaml
logging:
  level: "info"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `level` | string | `"info"` | Log level: `trace`, `debug`, `info`, `warn`, `error` |

**Environment override**: `NETWATCH_LOG_LEVEL`

### Discovery Configuration

```yaml
discovery:
  enabled: true
  interval: 300
  absence_multiplier: 3
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable network discovery |
| `interval` | u64 | `300` | Discovery interval in seconds |
| `absence_multiplier` | u64 | `3` | Absence detection threshold multiplier. An element is considered absent if `last_seen > interval * absence_multiplier` |

### Health Check Configuration (Legacy)

```yaml
health:
  interval: 15
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `interval` | u64 | `15` | Legacy ICMP health check interval (seconds) |

> **Note**: This is used only when `synthetic_checks.checks` is empty. Prefer `synthetic_checks` for new configurations.

### SNMP Configuration

```yaml
snmp:
  enabled: true
  interval: 60
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable SNMP polling (placeholder) |
| `interval` | u64 | `60` | SNMP polling interval in seconds |

> **Status**: SNMP collector is a placeholder - tables exist but no collection code implemented.

### Scheduler Configuration

```yaml
scheduler:
  concurrency: 10
  consecutive_failures_threshold: 3
  consecutive_successes_threshold: 1
  check_results_retention_days: 30
  events_retention_days: 90
  inventory_retention_days: 30
  retention_prune_interval: 3600
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `concurrency` | usize | `10` | Maximum concurrent scheduled jobs |
| `consecutive_failures_threshold` | u32 | `3` | Number of consecutive failures before emitting a down event/opening incident |
| `consecutive_successes_threshold` | u32 | `1` | Number of consecutive successes before emitting an up event/resolving incident (from confirmed down) |
| `check_results_retention_days` | u32 | `30` | Retention period for check_results in days |
| `events_retention_days` | u32 | `90` | Retention period for events in days |
| `inventory_retention_days` | u32 | `30` | Retention period for absent inventory (interfaces/routes) in days |
| `retention_prune_interval` | u64 | `3600` | Interval for retention pruning job in seconds |

### Capacity Planning Configuration

```yaml
capacity:
  enabled: true
  interval: 3600
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | Option<bool> | `Some(true)` | Enable capacity planning job |
| `interval` | Option<u64> | `Some(3600)` | Analysis interval in seconds |

### Anomaly Detection Configuration

```yaml
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
  anomaly_cooldown_seconds: 300
  min_absolute_change: 10.0
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | Option<bool> | `Some(true)` | Enable anomaly detection |
| `interval` | Option<u64> | `Some(300)` | Analysis interval in seconds |
| `min_samples` | Option<usize> | `Some(10)` | Minimum observations for baseline |
| `lookback_window_seconds` | Option<i64> | `Some(3600)` | Time window for baseline |
| `sensitivity` | Option<f64> | `Some(1.0)` | Detection sensitivity multiplier |
| `min_z_score` | Option<f64> | `Some(3.0)` | Minimum Z-score for anomalies |
| `enable_spike_detection` | Option<bool> | `Some(true)` | Enable spike detection |
| `enable_drift_detection` | Option<bool> | `Some(true)` | Enable drift detection |
| `enable_pattern_break_detection` | Option<bool> | `Some(false)` | Enable pattern break detection |
| `anomaly_cooldown_seconds` | Option<i64> | `Some(300)` | Cooldown between anomaly alerts |
| `min_absolute_change` | Option<f64> | `Some(10.0)` | Minimum absolute change for alerts |

### Alert Configuration

```yaml
alert:
  enabled: true
  interval: 60
  rules: []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | Option<bool> | `Some(true)` | Enable alert evaluation |
| `interval` | Option<u64> | `Some(60)` | Evaluation interval in seconds |
| `rules` | Vec<AlertRule> | `[]` | Alert rule definitions |

#### Alert Rule Structure

```yaml
rules:
  - id: "icmp_down"
    name: "ICMP Check Down"
    description: "Alert when ICMP check fails"
    enabled: true
    entity_type: "device"
    protocol: "icmp"
    condition:
      type: "down"
    severity: "critical"
    cooldown_seconds: 300
    labels:
      team: "network"
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique rule identifier |
| `name` | string | Yes | Human-readable name |
| `description` | string | No | Detailed description |
| `enabled` | bool | Yes | Enable/disable rule |
| `entity_type` | string | Yes | `"device"`, `"interface"`, or `"service"` |
| `protocol` | Option<string> | No | Filter by protocol (icmp, tcp, http, etc.) |
| `condition` | AlertCondition | Yes | Trigger condition |
| `severity` | AlertSeverity | Yes | `info`, `warning`, `critical` |
| `cooldown_seconds` | u64 | Yes | Minimum time between alerts |
| `labels` | map<string,string> | No | Additional labels |

#### Alert Conditions

| Type | Parameters | Description |
|------|------------|-------------|
| `Down` | (none) | Check is failing |
| `Up` | (none) | Check recovered |
| `LatencyThreshold` | `threshold_ms: f64` | Latency exceeds threshold |
| `LatencyPercentile` | `percentile: f64`, `threshold_ms: f64`, `window_seconds: i64` | Percentile latency exceeds threshold |
| `FailureRate` | `threshold: f64`, `window_seconds: i64` | Failure rate exceeds threshold |
| `SustainedDegradation` | `failure_count: u32`, `window_seconds: i64` | Consecutive failures |

### Synthetic Checks Configuration

```yaml
synthetic_checks:
  enabled: true
  interval: 60
  checks:
    - id: "web_frontend"
      name: "Web Frontend HTTP Check"
      check_type: "http"
      target: "example.com"
      expected_status: 200
      expected_body: "Welcome"
      headers:
        - ["User-Agent", "Netwatch"]
      timeout_ms: 5000
      enabled: true
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `enabled` | Option<bool> | No | Enable synthetic checks |
| `interval` | Option<u64> | No | Check interval in seconds |
| `checks` | Vec<SyntheticCheck> | No | List of checks |

#### Synthetic Check Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Unique check identifier |
| `name` | string | Yes | Human-readable name |
| `check_type` | SyntheticCheckType | Yes | `http`, `https`, `dns`, `tcp`, `tls` |
| `target` | string | Yes | Target hostname/IP |
| `port` | Option<u16> | No | Port (for tcp, tls) |
| `expected_status` | Option<u16> | No | Expected HTTP status |
| `expected_body` | Option<string> | No | Expected response body substring |
| `headers` | Option<Vec<[string,string]>> | No | HTTP headers |
| `timeout_ms` | Option<u64> | No | Timeout in milliseconds |
| `record_type` | Option<string> | No | DNS record type (A, AAAA, etc.) |
| `check_expiry_days` | Option<u64> | No | TLS cert expiry warning days |
| `verify_hostname` | Option<bool> | No | TLS hostname verification |
| `enabled` | bool | Yes | Enable/disable this check |

### Telegram Configuration

```yaml
telegram:
  enabled: false
  bot_token: "${TELEGRAM_BOT_TOKEN}"
  chat_id: "${TELEGRAM_CHAT_ID}"
  parse_mode: "HTML"
  timeout_seconds: 10
  max_retries: 3
  retry_backoff_seconds: 5
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Enable Telegram notifications |
| `bot_token` | string | `""` | Bot token from @BotFather |
| `chat_id` | string | `""` | Target chat ID |
| `parse_mode` | string | `"HTML"` | Message format |
| `timeout_seconds` | u64 | `10` | Request timeout |
| `max_retries` | u32 | `3` | Maximum retry attempts |
| `retry_backoff_seconds` | u64 | `5` | Base backoff for retries |

> **Security**: Use environment variables for secrets:
> ```yaml
> bot_token: "${TELEGRAM_BOT_TOKEN}"
> chat_id: "${TELEGRAM_CHAT_ID}"
> ```

### Authentication Configuration

```yaml
auth:
  enabled: true
  tokens: {}
  public_paths:
    - "/health"
    - "/metrics"
  token_header: "Authorization"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable token authentication |
| `tokens` | map<string,AuthUser> | `{}` | API tokens mapped to users (in production, use secure storage) |
| `public_paths` | Vec<string> | `["/health", "/metrics"]` | Paths that don't require authentication |
| `token_header` | string | `"Authorization"` | Token header name |

**Roles**: Administrator (full access), Operator (read/write), Viewer (read-only, MAC fields hidden)

### TLS Configuration

```yaml
tls:
  enabled: false
  cert_file: null
  key_file: null
  min_version: "1.2"
  require_client_auth: false
  ca_file: null
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Enable TLS (HTTPS) |
| `cert_file` | Option<string> | `null` | Path to certificate file (PEM format) |
| `key_file` | Option<string> | `null` | Path to private key file (PEM format) |
| `min_version` | string | `"1.2"` | Minimum TLS version |
| `require_client_auth` | bool | `false` | Require client certificate authentication (mTLS) |
| `ca_file` | Option<string> | `null` | Path to CA certificate file for client verification |

> **Status**: TLS on API is NOT implemented - server refuses to start if TLS configured (fail-closed). This is a security feature.

### SSRF Protection Configuration

```yaml
ssrf:
  enabled: true
  allow_private_ips: false
  allow_loopback: false
  allow_link_local: false
  allow_multicast: false
  allow_broadcast: false
  allowed_cidrs: []
  allowed_hostnames: []
  blocked_cidrs: []
  max_redirects: 10
  dns_timeout_ms: 5000
  validate_dns_rebinding: true
  allowed_ports: []
  blocked_ports: []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable SSRF protection |
| `allow_private_ips` | bool | `false` | Allow private IP ranges (RFC 1918) |
| `allow_loopback` | bool | `false` | Allow loopback addresses (127.0.0.0/8, ::1) |
| `allow_link_local` | bool | `false` | Allow link-local addresses (169.254.0.0/16, fe80::/10) |
| `allow_multicast` | bool | `false` | Allow multicast addresses |
| `allow_broadcast` | bool | `false` | Allow broadcast addresses |
| `allowed_cidrs` | Vec<string> | `[]` | Additional allowed CIDR ranges |
| `allowed_hostnames` | Vec<string> | `[]` | Additional allowed hostnames (exact match or suffix match with .) |
| `blocked_cidrs` | Vec<string> | `[]` | Blocked CIDR ranges (takes precedence over allowed) |
| `max_redirects` | u32 | `10` | Maximum number of redirects to follow |
| `dns_timeout_ms` | u64 | `5000` | Timeout for DNS resolution |
| `validate_dns_rebinding` | bool | `true` | Validate DNS resolution matches the original IP (prevent DNS rebinding) |
| `allowed_ports` | Vec<u16> | `[]` | Allowed ports (empty = all ports allowed) |
| `blocked_ports` | Vec<u16> | `[]` | Blocked ports (takes precedence over allowed) |

### Rate Limiting Configuration

```yaml
rate_limit:
  enabled: true
  max_requests: 100
  window_seconds: 60
  key_type: "ip"
  excluded_paths:
    - "/health"
    - "/metrics"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable rate limiting |
| `max_requests` | u32 | `100` | Maximum requests per window |
| `window_seconds` | u64 | `60` | Time window in seconds |
| `key_type` | string | `"ip"` | Key to use for rate limiting: "ip", "user", "token" |
| `excluded_paths` | Vec<string> | `["/health", "/metrics"]` | Paths to exclude from rate limiting |

## Environment Variable Overrides

The following environment variables override configuration file values:

| Variable | Config Path | Description |
|----------|-------------|-------------|
| `NETWATCH_STORAGE_PATH` | `storage.path` | SQLite database path |
| `NETWATCH_LOG_LEVEL` | `logging.level` | Log level |
| `NETWATCH_LISTEN` | `agent.listen` | HTTP bind address |

These are applied after YAML parsing, so they take precedence over file values.

## Configuration Loading

The application loads configuration in this order:

1. Parse CLI arguments (`--config` flag)
2. If `--config` provided, load that file
3. Otherwise use `Config::default()`
4. Apply environment variable overrides
5. Validate and use

## Creating a Custom Configuration

1. Copy the example:
   ```bash
   cp config.example.yaml netwatch.yaml
   ```

2. Edit `netwatch.yaml` with your settings

3. Run with custom config:
   ```bash
   ./target/release/netwatch --config netwatch.yaml
   ```

## Docker Configuration

In Docker Compose, the config is mounted as a volume:

```yaml
volumes:
  - ./config.example.yaml:/data/netwatch.yaml:ro
```

The container uses `CMD ["--config", "/data/netwatch.yaml"]` to load it.

### Docker Environment Variables

```yaml
environment:
  - NETWATCH_STORAGE_PATH=/data/netwatch.sqlite
  - NETWATCH_LOG_LEVEL=${NETWATCH_LOG_LEVEL:-info}
  - NETWATCH_LISTEN=0.0.0.0:8080
```

## Common Configuration Patterns

### Minimal Production Config

```yaml
agent:
  name: "netwatch-prod"
  listen: "127.0.0.1:8080"

storage:
  path: "/data/netwatch.sqlite"

metrics:
  enabled: true

api:
  enabled: true

logging:
  level: "warn"

discovery:
  enabled: true
  interval: 300

health:
  interval: 60

snmp:
  enabled: false
  interval: 60

scheduler:
  concurrency: 20
  consecutive_failures_threshold: 3
  consecutive_successes_threshold: 1
  check_results_retention_days: 30
  events_retention_days: 90
  inventory_retention_days: 30
  retention_prune_interval: 3600

capacity:
  enabled: true
  interval: 3600

anomaly:
  enabled: true
  interval: 300
  min_samples: 20
  lookback_window_seconds: 7200
  sensitivity: 1.5
  min_z_score: 3.5
  enable_spike_detection: true
  enable_drift_detection: true
  enable_pattern_break_detection: true
  anomaly_cooldown_seconds: 600
  min_absolute_change: 20.0

alert:
  enabled: true
  interval: 60
  rules:
    - id: "high_latency"
      name: "High Latency"
      enabled: true
      entity_type: "device"
      condition:
        type: "latency_threshold"
        threshold_ms: 1000
      severity: "warning"
      cooldown_seconds: 300

synthetic_checks:
  enabled: true
  interval: 60
  checks:
    - id: "api_health"
      name: "API Health Check"
      check_type: "https"
      target: "api.example.com/health"
      expected_status: 200
      timeout_ms: 5000
      enabled: true

telegram:
  enabled: true
  bot_token: "${TELEGRAM_BOT_TOKEN}"
  chat_id: "${TELEGRAM_CHAT_ID}"
```

### Development/Test Config

```yaml
agent:
  name: "netwatch-dev"
  listen: "127.0.0.1:8080"

storage:
  path: "./netwatch.sqlite"

metrics:
  enabled: true

api:
  enabled: true

logging:
  level: "debug"

discovery:
  enabled: false
  interval: 300

health:
  interval: 30

snmp:
  enabled: false
  interval: 60

scheduler:
  concurrency: 5

capacity:
  enabled: false
  interval: 3600

anomaly:
  enabled: false
  interval: 300

alert:
  enabled: false
  interval: 60
  rules: []

synthetic_checks:
  enabled: false
  interval: 60
  checks: []

telegram:
  enabled: false
```

## Validation

The configuration is validated at startup:

- Required fields must be present
- Types must match (string, integer, boolean)
- Enum values must be valid
- File paths must be accessible (for storage)

Invalid configuration will cause the application to exit with an error message.

## Troubleshooting

| Issue | Solution |
|-------|----------|
| "Could not read config" | Check file path and permissions |
| "Failed to parse YAML" | Validate YAML syntax with `yamllint` |
| "missing field" | Add required field or check spelling |
| Environment var not working | Ensure variable is exported before running |
| Database permission denied | Ensure `/data` is writable by container user |

## See Also

- [PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md) - Project architecture
- [API.md](API.md) - HTTP API reference
- [OPERATIONS.md](OPERATIONS.md) - Operational procedures
- [README.md](../README.md) - Quick start guide