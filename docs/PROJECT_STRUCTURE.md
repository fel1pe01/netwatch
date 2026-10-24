# Netwatch Project Structure

This document provides a comprehensive overview of the Netwatch project structure, explaining the purpose and relationships of each component.

## Directory Tree

```
netwatch/
├── Cargo.toml                 # Rust project manifest with dependencies
├── Cargo.lock                 # Locked dependency versions
├── README.md                  # Main project documentation
├── compose.yaml               # Docker Compose for full observability stack
├── Dockerfile                 # Multi-stage Docker build
├── config.example.yaml        # Example configuration with all options
├── netwatch.yaml              # Local development config (gitignored)
├── test-config.yaml           # Test configuration (gitignored)
├── Dockerfile.backup          # Backup of original Dockerfile
├── src/                       # Rust source code
│   ├── main.rs                # Application entry point
│   ├── lib.rs                 # Library exports and module declarations
│   ├── config.rs              # Configuration structures and loading
│   ├── metrics.rs             # Application-level metrics (info, uptime)
│   ├── metrics_handler_test.rs # Metrics handler tests
│   ├── config_file_test.rs    # Configuration parsing tests
│   ├── logging.rs             # Logging initialization
│   ├── storage.rs             # SQLite storage layer and queries
│   ├── scheduler.rs           # Job scheduler implementation
│   ├── events.rs              # Event bus for inter-component communication
│   ├── api.rs                 # HTTP API endpoints
│   ├── alerts.rs              # Alert engine and evaluation
│   ├── notifications.rs       # Notification manager (Telegram)
│   ├── incidents.rs           # Incident management
│   ├── anomaly.rs             # Anomaly detection
│   ├── capacity_planning.rs   # Capacity analysis and reporting
│   ├── drift.rs               # Configuration drift detection
│   ├── root_cause.rs          # Root cause analysis
│   ├── topology.rs            # Topology data structures
│   ├── topology_builder.rs    # Topology graph construction
│   ├── topology_store.rs      # Topology persistence
│   ├── service.rs             # Service status tracking
│   ├── snmp.rs                # SNMP collector (placeholder)
│   ├── discovery/             # Network discovery modules
│   │   ├── mod.rs
│   │   ├── interfaces.rs      # Network interface discovery
│   │   └── routing.rs         # Routing table discovery
│   └── monitoring/            # Health check implementations
│       ├── mod.rs             # Exports and common types
│       ├── metrics.rs         # Prometheus metrics definitions
│       ├── icmp.rs            # ICMP ping checks
│       ├── tcp.rs             # TCP connection checks
│       ├── dns.rs             # DNS resolution checks
│       ├── http.rs            # HTTP/HTTPS checks with phase timing
│       └── tls.rs             # TLS certificate checks
├── migrations/                # SQL migrations for SQLite
│   ├── 20231006000000_initial.sql
│   ├── 20241009000000_topology.sql
│   ├── 20241009000001_incidents.sql
│   ├── 20241219000000_service_status.sql
│   └── 20251011000000_root_cause.sql
├── tests/                     # Integration tests
│   ├── integration_test.rs
│   ├── cli_tests.rs
│   ├── discovery_tests.rs
│   ├── monitoring_tests.rs
│   ├── http_integration_tests.rs
│   ├── metrics_cardinality_test.rs
│   ├── config_file_test.rs
│   ├── drift_tests.rs
│   ├── anomaly_tests.rs
│   ├── capacity_tests.rs
│   ├── incident_tests.rs
│   └── root_cause_tests.rs
├── scripts/                   # Operational scripts
│   ├── e2e_smoke.sh           # End-to-end smoke test
│   └── check.sh               # Pre-push validation script
├── docs/                      # Documentation
│   ├── ARCHITECTURE.md
│   ├── ARCHITECTURE_AND_FUNCTIONAL_TESTS.md
│   ├── CHANGES_MADE.md
│   ├── DEVELOPMENT.md
│   ├── IMPLEMENTATION_STATUS.md
│   ├── IMPLEMENTATION_STATUS.md
│   ├── ROADMAP.md
│   ├── PROJECT_STRUCTURE.md   # This file
│   ├── CONFIGURATION.md
│   ├── API.md
│   ├── OPERATIONS.md
│   ├── TESTING.md
│   ├── audits/
│   └── checkpoints/
├── prometheus/                # Prometheus configuration
│   ├── prometheus.yml         # Main Prometheus config
│   └── rules/                 # Alerting rules
│       └── netwatch.yml       # Netwatch-specific alerts
├── grafana/                   # Grafana provisioning
│   ├── provisioning/
│   │   ├── datasources.yml    # Prometheus datasource
│   │   └── dashboards/        # Dashboard provisioning
│   │       └── dashboards.yml
│   └── dashboards/
│       └── netwatch-overview.json  # Main dashboard
├── alertmanager/              # Alertmanager configuration
│   └── alertmanager.yml       # Alert routing and receivers
└── scripts/                   # (duplicate entry, see above)
```

## Component Responsibilities

### Core Application (`src/`)

| Module | Responsibility |
|--------|----------------|
| `main.rs` | Application entry point, CLI parsing, server startup, component wiring |
| `lib.rs` | Public API exports, module declarations |
| `config.rs` | Configuration structures, YAML parsing, environment variable overrides |
| `metrics.rs` | Application-level Prometheus metrics (netwatch_info, netwatch_uptime_seconds) |
| `storage.rs` | SQLite connection, migrations, observation persistence, queries |
| `scheduler.rs` | Recurring job scheduler with configurable concurrency |
| `events.rs` | In-memory event bus for decoupled component communication |
| `api.rs` | HTTP endpoints (health, metrics, monitoring, devices, topology) |
| `alerts.rs` | Alert rule evaluation engine with cooldown and deduplication |
| `notifications.rs` | Multi-channel notification manager (Telegram implementation) |
| `incidents.rs` | Incident lifecycle management with state machine |
| `anomaly.rs` | Statistical anomaly detection (spike, drift, pattern break) |
| `capacity_planning.rs` | Trend analysis, time-to-saturation projections |
| `drift.rs` | Configuration drift detection and reporting |
| `root_cause.rs` | Dependency graph analysis for root cause hypotheses |
| `topology.rs` | Graph data structures for network topology |
| `topology_builder.rs` | Builds topology from discovery and SNMP data |
| `topology_store.rs` | Persists topology snapshots and changes |
| `service.rs` | Service entity with independent status tracking |
| `snmp.rs` | SNMP data collection (placeholder - not implemented) |
| `discovery/` | Network interface and routing table discovery |
| `monitoring/` | Health check implementations and metrics |

### Configuration Files

| File | Purpose |
|------|---------|
| `config.example.yaml` | Complete example with all options documented |
| `test-config.yaml` | Minimal config for testing (discovery disabled) |
| `netwatch.yaml` | Local development config (not versioned) |

### Infrastructure (`compose.yaml`)

Services:
- **netwatch**: Core application (profiles: core, full)
- **prometheus**: Metrics collection (profiles: observability, full)
- **grafana**: Visualization (profiles: observability, full)
- **alertmanager**: Alert routing (profiles: observability, full)
- **kafka**: Event streaming (profiles: messaging, full)
- **kafka-ui**: Kafka monitoring (profiles: messaging, full)

### Monitoring Stack

| Component | Config | Purpose |
|-----------|--------|---------|
| Prometheus | `prometheus/prometheus.yml` | Scrapes netwatch:8080/metrics, loads rules from `prometheus/rules/` |
| Grafana | `grafana/provisioning/` | Auto-provisions Prometheus datasource and Netwatch dashboard |
| Alertmanager | `alertmanager/alertmanager.yml` | Routes alerts to webhook receivers, supports Telegram |

## Data Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Scheduler  │────▶│  Monitoring │────▶│  Storage    │
│  (jobs)     │     │  (checks)   │     │  (SQLite)   │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       ▼                   ▼                   ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Events     │     │  Metrics    │     │  Queries    │
│  (bus)      │     │  (Prometheus)│    │  (API)      │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       ▼                   ▼                   ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Alerts     │     │  Prometheus │     │  Grafana    │
│  Engine     │     │  (scrape)   │     │  (dashboards)│
└─────────────┘     └─────────────┘     └─────────────┘
```

## Key Relationships

1. **Scheduler → Monitoring**: Scheduler runs health check jobs at configured intervals
2. **Monitoring → Storage**: Check results persisted as observations
3. **Monitoring → Metrics**: Results update Prometheus counters/histograms
4. **Monitoring → Events**: Check completion emits events to EventBus
5. **Events → Alerts**: AlertEngine evaluates rules against observations
6. **Events → Incidents**: IncidentEngine creates/manages incidents
7. **Storage → API**: API endpoints query SQLite for devices, observations, incidents
8. **Metrics → Prometheus**: Prometheus scrapes `/metrics` endpoint
9. **Prometheus → Grafana**: Dashboards query Prometheus for visualization
10. **Alerts → Alertmanager**: Firing alerts sent to webhook receivers

## Points of Entry

| Entry Point | File | Description |
|-------------|------|-------------|
| CLI | `src/main.rs` | `netwatch` binary with subcommands |
| HTTP Server | `src/main.rs` | Axum server on configured address |
| Metrics | `src/metrics.rs` | `/metrics` endpoint for Prometheus |
| Health | `src/api.rs` | `/health` endpoint for Docker healthcheck |
| Config | `src/config.rs` | `Config::from_path()` or `Config::default()` |

## Implemented vs Placeholder

| Feature | Status | Notes |
|---------|--------|-------|
| Core monitoring (ICMP, TCP, DNS, HTTP, TLS) | ✅ Implemented | Full health checks with metrics |
| Synthetic checks | ✅ Implemented | Config-driven with per-phase metrics |
| Scheduler | ✅ Implemented | Multi-job, concurrent, graceful shutdown |
| SQLite persistence | ✅ Implemented | Migrations, observations, incidents |
| Alert engine | ✅ Implemented | 6 rule types, cooldown, dedup |
| Incident management | ✅ Implemented | State machine, auto-resolve, correlation |
| Anomaly detection | ✅ Implemented | Spike, drift, pattern break |
| Capacity planning | ✅ Implemented | Linear regression, reports |
| Topology discovery | ✅ Implemented | Interfaces, routes, graph export |
| Drift detection | ✅ Implemented | Expected vs actual comparison |
| Root cause analysis | ✅ Implemented | Hypothesis model, confirmation API |
| SNMP monitoring | ⚠️ Placeholder | Tables exist, no collector code |
| Telegram notifications | ✅ Implemented | Rate limited, retries, HTML |
| Alertmanager webhook | ✅ Implemented | Receives Alertmanager payloads |
| API authentication | ❌ Not implemented | No auth/TLS on API |
| Config hot-reload | ❌ Not implemented | Requires restart |
| Kafka integration | ⚠️ Placeholder | Config only, no producer/consumer |

## Testing

| Test Suite | Location | Coverage |
|------------|----------|----------|
| Unit tests | `src/*_test.rs`, `tests/*.rs` | 93 tests |
| E2E smoke | `scripts/e2e_smoke.sh` | 40 checks |
| Config tests | `src/config_file_test.rs` | YAML parsing, env overrides |
| Metrics tests | `tests/metrics_cardinality_test.rs` | Cardinality enforcement |
| HTTP tests | `tests/http_integration_tests.rs` | API endpoints, metrics |
| CLI tests | `tests/cli_tests.rs` | Command-line interface |

## Build & Deploy

```bash
# Local development
cargo build --release
./target/release/netwatch --config config.example.yaml

# Docker
docker build -t netwatch .
docker run -p 8085:8080 -v $(pwd)/config.example.yaml:/data/netwatch.yaml netwatch

# Full stack
docker compose --profile full up -d
```

## Related Documentation

- [README.md](../README.md) - Quick start and overview
- [CONFIGURATION.md](CONFIGURATION.md) - Configuration guide
- [API.md](API.md) - HTTP API reference
- [OPERATIONS.md](OPERATIONS.md) - Operational procedures
- [TESTING.md](TESTING.md) - Testing guide
- [ARCHITECTURE.md](ARCHITECTURE.md) - Architecture deep dive
- [ROADMAP.md](ROADMAP.md) - Strategic objectives
- [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) - Per-objective status
- [CHANGES_MADE.md](CHANGES_MADE.md) - Change log