# Netwatch Operations Guide

This guide covers operational procedures for deploying, monitoring, and maintaining Netwatch.

## Table of Contents

1. [Deployment](#deployment)
2. [Running Locally](#running-locally)
3. [Docker Deployment](#docker-deployment)
4. [Configuration Management](#configuration-management)
5. [Monitoring Netwatch](#monitoring-netwatch)
6. [Log Analysis](#log-analysis)
7. [Database Management](#database-management)
8. [Troubleshooting](#troubleshooting)
9. [Backup and Recovery](#backup-and-recovery)
10. [Upgrading](#upgrading)

## Deployment

### Prerequisites

- Rust 1.75+ (for building from source)
- Docker 20.10+ and Docker Compose 2.0+ (for containerized deployment)
- SQLite 3.35+ (included in Docker image)
- Linux (required for network discovery)

### Building from Source

```bash
# Clone repository
git clone https://github.com/fel1pe01/netwatch
cd netwatch

# Build release binary
cargo build --release

# Binary location
./target/release/netwatch
```

### Docker Image

```bash
# Build image
docker build -t netwatch .

# Run container
docker run -d \
  --name netwatch \
  -p 8085:8080 \
  -v $(pwd)/config.example.yaml:/data/netwatch.yaml:ro \
  -v netwatch-data:/data \
  netwatch
```

## Running Locally

### Quick Start

```bash
# Build
cargo build --release

# Create config
cp config.example.yaml netwatch.yaml
# Edit netwatch.yaml as needed

# Run server
./target/release/netwatch --config netwatch.yaml

# Test CLI
./target/release/netwatch monitor ping 1.2.3.4
./target/release/netwatch discover local
```

### With Custom Config

```bash
# Using custom config file
./target/release/netwatch --config /path/to/custom.yaml

# Using environment variables
NETWATCH_STORAGE_PATH=/tmp/netwatch.sqlite \
NETWATCH_LOG_LEVEL=debug \
./target/release/netwatch
```

### Verify Installation

```bash
# Health check
curl http://localhost:8080/health
# {"status":"ok"}

# Metrics
curl http://localhost:8080/metrics

# API test
curl -X POST http://localhost:8080/api/v1/monitor/ping \
  -H "Content-Type: application/json" \
  -d '{"target":"1.2.3.4"}'
```

## Docker Deployment

### Docker Compose Profiles

The compose.yaml defines four profiles:

| Profile | Services | Use Case |
|---------|----------|----------|
| `core` | netwatch | Minimal monitoring |
| `observability` | netwatch, prometheus, grafana, alertmanager | Full monitoring stack |
| `messaging` | netwatch, kafka, kafka-ui | Event streaming |
| `full` | All services | Complete stack |

### Starting the Stack

```bash
# Core only
docker compose --profile core up -d

# Full observability stack
docker compose --profile observability up -d

# Complete stack
docker compose --profile full up -d

# Custom config
docker compose --profile observability up -d \
  -f compose.yaml \
  -f docker-compose.override.yaml
```

### Port Mapping

| Service | Internal Port | Host Port (default) | Description |
|---------|--------------|---------------------|-------------|
| Netwatch | 8080 | 8085 | HTTP API & metrics |
| Prometheus | 9090 | 9090 | Metrics UI |
| Grafana | 3000 | 3000 | Dashboards |
| Alertmanager | 9093 | 9093 | Alert UI |
| Kafka | 9092 | 9092 | Message broker |
| Kafka UI | 8080 | 8081 | Kafka UI |

### Customizing Ports

```bash
# Override via environment
NETWATCH_PORT=9080 \
PROMETHEUS_PORT=9091 \
GRAFANA_PORT=3001 \
docker compose --profile observability up -d
```

### Persistent Data

| Volume | Purpose |
|--------|---------|
| `netwatch-data` | SQLite database, config |
| `prometheus-data` | Metrics storage |
| `grafana-data` | Dashboards, settings |
| `alertmanager-data` | Alert state |
| `kafka-data` | Message logs |

## Configuration Management

### Configuration Files

| File | Purpose | Versioned |
|------|---------|-----------|
| `config.example.yaml` | Full example with all options | Yes |
| `test-config.yaml` | Minimal test config | No (local) |
| `netwatch.yaml` | Local development config | No (gitignored) |

### Environment Variables

| Variable | Config Path | Description |
|----------|-------------|-------------|
| `NETWATCH_STORAGE_PATH` | `storage.path` | SQLite database path |
| `NETWATCH_LOG_LEVEL` | `logging.level` | Log level (trace/debug/info/warn/error) |
| `NETWATCH_LISTEN` | `agent.listen` | HTTP bind address |

### Secrets Management

**Never commit secrets to git.** Use environment variables:

```yaml
# config.example.yaml
telegram:
  enabled: true
  bot_token: "${TELEGRAM_BOT_TOKEN}"
  chat_id: "${TELEGRAM_CHAT_ID}"
```

```bash
# Runtime
export TELEGRAM_BOT_TOKEN="123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
export TELEGRAM_CHAT_ID="-1001234567890"
docker compose up -d
```

### Configuration Validation

The application validates config at startup:
- Required fields present
- Correct types
- Valid enum values
- Accessible file paths

## Monitoring Netwatch

### Health Checks

```bash
# Liveness probe
curl http://localhost:8080/health
# {"status":"ok"}

# With Docker
docker exec netwatch wget -qO- http://localhost:8080/health
```

### Metrics Endpoint

```bash
# Prometheus metrics
curl http://localhost:8080/metrics

# Key metrics to monitor
curl -s http://localhost:8080/metrics | grep -E \
  'netwatch_health_checks_total|netwatch_uptime_seconds|netwatch_scheduler_jobs_total'
```

### Key Metrics

| Metric | Type | Description | Alert Threshold |
|--------|------|-------------|-----------------|
| `up{job="netwatch"}` | gauge | Service up (1) or down (0) | `== 0` for 1m |
| `netwatch_uptime_seconds` | gauge | Uptime in seconds | N/A |
| `netwatch_health_checks_total` | counter | Total checks by protocol | N/A |
| `netwatch_health_check_failures_total` | counter | Failed checks by protocol | Rate > 0.1 |
| `netwatch_health_check_latency_seconds` | histogram | Check latency | p95 > 1s |
| `netwatch_scheduler_jobs_total` | counter | Jobs started by job name | N/A |
| `netwatch_scheduler_errors_total` | counter | Job errors by job name | Rate > 0 |

### Prometheus Integration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'netwatch'
    static_configs:
      - targets: ['netwatch:8080']
    metrics_path: '/metrics'
    scrape_interval: 10s
```

### Grafana Dashboards

Access Grafana at `http://localhost:3000` (default: admin/admin)

Pre-provisioned dashboard: **Netwatch Overview**

Panels:
- **Netwatch Status**: `up{job="netwatch"}` (green=up, red=down)
- **Uptime**: `netwatch_uptime_seconds`
- **Health Check Rate**: `sum(rate(netwatch_health_checks_total[5m]))`
- **Failure Rate**: `rate(failures)/rate(total)`
- **Latency Percentiles**: p50/p95/p99 by protocol
- **Rate by Protocol**: Total and failures per protocol
- **Active Alerts**: Firing Alertmanager alerts
- **Scheduler Jobs**: `netwatch_scheduler_jobs_total`

### Alertmanager

Access Alertmanager at `http://localhost:9093`

Default routes:
- **Critical**: Immediate, repeat 1h
- **Warning**: 30s wait, repeat 4h
- **Info**: 24h repeat

Webhook receiver: `http://netwatch:8080/api/v1/alerts/webhook`

## Log Analysis

### Log Format

```text
2024-01-15T10:30:00.123456Z INFO netwatch: Running service checks...
2024-01-15T10:30:00.456789Z DEBUG sqlx::query: summary="SELECT ..."
2024-01-15T10:30:00.789012Z ERROR netwatch: Failed to persist observation: ...
```

### Log Levels

| Level | Use Case |
|-------|----------|
| `trace` | Detailed SQL, internal state |
| `debug` | SQL queries, scheduler ticks |
| `info` | Job starts, check results (default) |
| `warn` | Retries, degraded performance |
| `error` | Failed checks, persistence errors |

### Viewing Logs

```bash
# Local
./target/release/netwatch --config netwatch.yaml 2>&1 | tee netwatch.log

# Docker
docker logs -f netwatch

# With timestamps
docker logs -f -t netwatch

# Filter errors
docker logs netwatch 2>&1 | grep ERROR
```

### Structured Logging

Logs use `tracing` crate with structured fields:

```json
{
  "timestamp": "2024-01-15T10:30:00.123Z",
  "level": "INFO",
  "target": "netwatch",
  "message": "Running service checks...",
  "fields": {
    "job": "service_check",
    "checks_due": 5
  }
}
```

## Database Management

### SQLite Database

Location: Configured via `storage.path` (default: `netwatch.sqlite`)

### Schema

Run migrations automatically at startup:
```bash
# Migrations in migrations/ directory
# Applied automatically on startup
```

### Manual Queries

```bash
# Connect
sqlite3 /data/netwatch.sqlite

# Common queries
.tables
.schema devices
SELECT * FROM devices LIMIT 10;
SELECT * FROM observations ORDER BY observed_at DESC LIMIT 20;
SELECT * FROM incidents WHERE state = 'open';
```

### Vacuum and Maintenance

```bash
# Vacuum (reclaim space)
sqlite3 /data/netwatch.sqlite "VACUUM;"

# Analyze (update statistics)
sqlite3 /data/netwatch.sqlite "ANALYZE;"

# Check integrity
sqlite3 /data/netwatch.sqlite "PRAGMA integrity_check;"
```

### WAL Mode

The database uses WAL (Write-Ahead Logging) mode for better concurrency:
- `-wal` and `-shm` files created alongside database
- Do not delete these files while running

## Troubleshooting

### Common Issues

#### Container Restart Loop

**Symptoms:** Container restarts repeatedly, `docker ps` shows `Restarting (1)`

**Causes:**
1. Config parse error - check logs: `docker logs netwatch`
2. Database permission - ensure `/data` is writable
3. Port conflict - check `NETWATCH_LISTEN` and port mapping

**Debug:**
```bash
docker logs netwatch --tail 50
```

#### Prometheus Target DOWN

**Symptoms:** Prometheus target shows `DOWN`, error: `lookup netwatch on 127.0.0.11:53: no such host`

**Causes:**
1. Netwatch container not running (check restart loop above)
2. Wrong target in prometheus.yml - should be `netwatch:8080`
3. Services not on same Docker network

**Fix:**
```yaml
# prometheus.yml - use service name and internal port
targets: ['netwatch:8080']
```

#### Grafana Shows "No Data"

**Symptoms:** Panels show "No data" or "N/A"

**Causes:**
1. Netwatch not running or metrics not exposed
2. Wrong Prometheus datasource URL
3. Time range too narrow
4. Metric names don't match

**Debug:**
```bash
# Check metrics endpoint
curl http://netwatch:8080/metrics | grep netwatch_

# Check Prometheus target
curl http://prometheus:9090/api/v1/targets
```

#### Metrics Not Appearing

**Symptoms:** Expected metrics missing from `/metrics`

**Causes:**
1. Feature not enabled in config (e.g., `discovery.enabled: false`)
2. No checks have run yet (scheduler interval not elapsed)
3. Metric registration failed (check startup logs)

#### Database Locked

**Symptoms:** `database is locked` errors

**Causes:**
1. Multiple processes accessing same SQLite file
2. WAL files not cleaned up
3. Long-running transaction

**Fix:**
```bash
# Ensure single process
# Check for -wal/-shm files
ls -la /data/netwatch.sqlite*

# Restart to clear locks
docker restart netwatch
```

### Debug Mode

```bash
# Enable debug logging
NETWATCH_LOG_LEVEL=debug ./target/release/netwatch --config config.yaml

# Docker
docker run -e NETWATCH_LOG_LEVEL=debug netwatch
```

## Backup and Recovery

### Backup Strategy

```bash
# Online backup (SQLite supports online backup)
sqlite3 /data/netwatch.sqlite ".backup /backup/netwatch_$(date +%F).sqlite"

# Or copy files (ensure no writes)
docker stop netwatch
cp /data/netwatch.sqlite /backup/netwatch_$(date +%F).sqlite
docker start netwatch
```

### Automated Backup

```bash
#!/bin/bash
# /etc/cron.daily/netwatch-backup
BACKUP_DIR="/backup/netwatch"
mkdir -p "$BACKUP_DIR"
sqlite3 /data/netwatch.sqlite ".backup $BACKUP_DIR/netwatch_$(date +%F).sqlite"
# Keep 30 days
find "$BACKUP_DIR" -name "netwatch_*.sqlite" -mtime +30 -delete
```

### Recovery

```bash
# Stop service
docker stop netwatch

# Restore
cp /backup/netwatch_2024-01-15.sqlite /data/netwatch.sqlite

# Start service
docker start netwatch
```

### Point-in-Time Recovery

Not supported with SQLite (no WAL archiving). Use frequent backups.

## Upgrading

### From Source

```bash
git pull origin main
cargo build --release
# Stop service, replace binary, restart
```

### Docker

```bash
# Pull latest image
docker compose pull

# Rebuild if building locally
docker compose build --no-cache

# Restart
docker compose up -d
```

### Database Migrations

Migrations run automatically on startup:
- Located in `migrations/`
- Applied in version order
- Idempotent (safe to re-run)

```bash
# Check migration status
sqlite3 /data/netwatch.sqlite "SELECT * FROM _sqlx_migrations;"
```

### Rolling Upgrade (Zero Downtime)

Not currently supported (single instance). Plan maintenance window.

## Performance Tuning

### Scheduler Concurrency

```yaml
scheduler:
  concurrency: 20  # Increase for many checks
```

### Database

- Use SSD storage for `/data`
- Enable WAL mode (default)
- Regular `VACUUM` and `ANALYZE`

### Prometheus

```yaml
# Reduce scrape interval for less load
scrape_interval: 30s  # default 15s
```

### Grafana

- Use recording rules for complex queries
- Limit dashboard time ranges
- Disable auto-refresh for large dashboards

## Security Considerations

### Network

- Bind to `0.0.0.0` only in trusted networks
- Use reverse proxy (nginx, Traefik) for TLS termination
- Restrict Prometheus/Grafana/Alertmanager to internal networks

### API

- No authentication currently implemented
- Use network policies / firewall rules
- Consider adding auth proxy (oauth2-proxy, Authelia)

### Secrets

- Never commit secrets to git
- Use environment variables or Docker secrets
- Rotate Telegram bot tokens periodically

### Database

- SQLite file permissions: `600` (owner read/write only)
- Directory permissions: `700`
- Run as non-root user (Dockerfile creates `netwatch` user)

## Maintenance Checklist

### Daily
- [ ] Check container health: `docker ps`
- [ ] Verify metrics: `curl /metrics`
- [ ] Check logs for errors: `docker logs netwatch`

### Weekly
- [ ] Run database maintenance: `VACUUM`, `ANALYZE`
- [ ] Check disk space: `df -h /data`
- [ ] Review alert rules and firing alerts

### Monthly
- [ ] Update dependencies: `cargo update`
- [ ] Review and rotate secrets
- [ ] Test backup recovery
- [ ] Update Docker base images

## See Also

- [CONFIGURATION.md](CONFIGURATION.md) - Configuration reference
- [API.md](API.md) - API reference
- [README.md](../README.md) - Quick start
- [TESTING.md](TESTING.md) - Testing procedures