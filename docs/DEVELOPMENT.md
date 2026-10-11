# Development Guide

## Project structure

```
netwatch/
├── Cargo.toml
├── README.md
├── compose.yaml
├── Dockerfile
├── config.example.yaml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config.rs
│   ├── logging.rs
│   ├── storage.rs
│   ├── api.rs
│   ├── metrics.rs
│   ├── scheduler.rs
│   ├── events.rs
│   ├── incidents.rs
│   ├── drift.rs
│   ├── topology.rs
│   ├── topology_builder.rs
│   ├── topology_store.rs
│   ├── snmp.rs
│   ├── config_file_test.rs
│   ├── metrics_handler_test.rs
│   ├── monitoring/
│   │   ├── mod.rs
│   │   ├── icmp.rs
│   │   ├── tcp.rs
│   │   ├── dns.rs
│   │   ├── http.rs
│   │   ├── tls.rs
│   │   └── metrics.rs
│   ├── discovery/
│   │   ├── mod.rs
│   │   ├── interfaces.rs
│   │   └── routing.rs
│   └── topology/
│       ├── mod.rs
│       ├── builder.rs
│       └── store.rs
├── migrations/
│   ├── 20231006000000_initial.sql
│   └── 20241009000000_topology.sql
├── tests/
│   ├── integration_test.rs
│   ├── cli_tests.rs
│   ├── discovery_tests.rs
│   ├── monitoring_tests.rs
│   ├── http_integration_tests.rs
│   ├── metrics_cardinality_test.rs
│   ├── config_file_test.rs
│   └── drift_tests.rs
├── scripts/
│   └── e2e_smoke.sh
├── docs/
│   ├── ROADMAP.md
│   ├── ARCHITECTURE.md
│   ├── IMPLEMENTATION_STATUS.md
│   ├── ARCHITECTURE_AND_FUNCTIONAL_TESTS.md
│   ├── CHANGES_MADE.md
│   └── DEVELOPMENT.md
├── prometheus/
│   └── prometheus.yml
├── grafana/
│   ├── provisioning/
│   │   ├── datasources.yml
│   │   └── dashboards.yml
│   └── dashboards/
│       └── netwatch-overview.json
├── alertmanager/
│   └── alertmanager.yml
├── compose.yaml
└── Dockerfile
```

All core functionality is split into small, single‑purpose modules.

## Running unit tests

```bash
cargo test
```

The tests include (46 tests passing):
- Configuration parsing (5 tests).
- SQLite migration and basic queries (2 tests).
- Scheduler behaviour (job scheduling, timeout, concurrency) — in integration_test.rs.
- Discovery (interfaces, routes) (2 tests).
- Monitoring (ICMP ping, TCP checks) (3 tests).
- HTTP integration tests (13 tests).
- Metrics cardinality enforcement (9 tests).
- Config file loading (5 tests).
- CLI integration tests (6 tests).
- Metrics handler (1 test).
- Drift detection (5 tests).

Total: 46 unit/integration tests + 42 E2E smoke test checks.

## Building for release

```bash
cargo build --release
```

The binary will be available at `target/release/netwatch`.

## Code Quality Checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

## E2E Smoke Test

```bash
./scripts/e2e_smoke.sh
```

Runs 42 checks against the release binary (5 CLI, 12 HTTP, 10 metrics, 4 drift, 11 topology).

## Configuration

Create a `config.yaml` based on `config.example.yaml`:

```yaml
agent:
  name: "netwatch"
  listen: "0.0.0.0:8080"

storage:
  path: "netwatch.sqlite"

metrics:
  enabled: true
  path: "metrics.sqlite"

api:
  enabled: true

logging:
  level: "info"

discovery:
  enabled: true
  interval: 300

health:
  interval: 15

ports:
  enabled: true
  interval: 900

snmp:
  enabled: false
  interval: 60

scheduler:
  concurrency: 10
  rate_limit: 1000
```

Run with config:
```bash
./target/release/netwatch --config config.yaml
```

Or use environment variables:
```bash
NETWATCH_STORAGE_PATH=/data/netwatch.sqlite NETWATCH_LOG_LEVEL=debug ./target/release/netwatch
```