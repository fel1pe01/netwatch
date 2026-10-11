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
│   ├── config_file_test.rs
│   ├── metrics_handler_test.rs
│   ├── snmp.rs
│   ├── topology.rs
│   ├── topology_builder.rs
│   ├── topology_store.rs
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
│   └── config_file_test.rs
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

The tests include (41 tests passing):
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
- Config file loading tests (5 tests).

Total: 41 unit/integration tests + 42 E2E smoke test checks.

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

Runs 42 checks against the release binary (5 CLI, 12 HTTP, 10 metrics).
