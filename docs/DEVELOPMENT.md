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

The tests include (91 tests passing in last verified run):
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
- Anomaly detection (13 tests).
- Capacity planning (11 tests).
- Incident management (12 tests).
- Root cause analysis (8 tests).

**Note on test counts:** The "91 tests" above includes all unit and integration tests across all test files (46 base tests + 13 anomaly + 11 capacity + 12 incident + 8 root cause + 1 metrics handler = ~91). Other documents may reference "46 unit/integration tests" as the core suite excluding the newer P4 test modules (anomaly, capacity, incident, root_cause). Both counts are correct for different categorizations.

**Test verification status:** Tests were last fully verified on 2026-10-13. Since then, `cargo test`, `cargo clippy`, and `cargo build --release` are blocked locally by a toolchain issue (missing `dlltool.exe` in the GNU/MinGW target on Windows). CI (GitHub Actions, Ubuntu/MSVC) executes the full suite on every push. Run `./scripts/check.sh --skip-audit --skip-docker --skip-e2e` for fmt/check only, or install MSVC target (`rustup target add x86_64-pc-windows-msvc`) to restore full local verification.

Total: 91 unit/integration tests + 42 E2E smoke test checks.

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

## Security Validation

```bash
cargo audit
cargo deny check advisories bans licenses sources
```

Both commands pass. One advisory (RUSTSEC-2023-0071) is documented as an exception — see [CHANGES_MADE.md](docs/CHANGES_MADE.md) for justification.

## E2E Smoke Test

```bash
./scripts/e2e_smoke.sh
```

Runs 42 checks against the release binary (5 CLI, 12 HTTP, 10 metrics, 4 drift, 11 topology).

## Pre-push Validation (Local CI Replica)

The repository includes `scripts/check.sh`, a comprehensive validation script that mirrors the CI pipeline. Run it before pushing:

```bash
./scripts/check.sh
```

### Docker Validation Behavior

The Docker step in `check.sh` automatically manages the `netwatch:check` validation image:

- **Image exists**: Deletes it first, then rebuilds from scratch with `docker build --pull -t netwatch:check .`
- **Image absent**: Builds directly
- **Build failure**: Recorded as FAIL in the summary with the build log shown
- **Cleanup**: The validation image is removed after the check (best-effort)

This ensures the Docker build is always tested from a clean state, matching CI behavior.

### Docker Desktop on WSL2

For the Docker validation to work in WSL2:

1. Install **Docker Desktop for Windows**
2. Enable **WSL 2 integration** in Docker Desktop Settings → Resources → WSL Integration
3. Ensure your Ubuntu distro is toggled on
4. The `docker` CLI in WSL will then connect to the Docker Desktop daemon

If Docker is unavailable or you want to skip the Docker build:

```bash
./scripts/check.sh --skip-docker
```

This records an explicit skip (exit code 0) and does not mark the validation as incomplete.

### Validation Modes

| Mode | Command | Exit Code | Use Case |
|------|---------|-----------|----------|
| Full (pre-push) | `./scripts/check.sh` | 0=pass, 1=fail, 3=incomplete | Default; runs everything from clean |
| Quick iteration | `./scripts/check.sh --quick` | 3 (incomplete) | fmt, check, test, clippy only |
| Skip Docker | `./scripts/check.sh --skip-docker` | 0/1/3 | No Docker / WSL without Docker Desktop |
| Skip security | `./scripts/check.sh --skip-audit` | 0/1/3 | No cargo-audit/deny installed |
| Skip E2E | `./scripts/check.sh --skip-e2e` | 0/1/3 | No release binary / slow environment |

See `./scripts/check.sh --help` for all options.

## Configuration

Create a `config.yaml` based on `config.example.yaml`:

```yaml
agent:
  name: "netwatch"
  listen: "0.0.0.0:8080"

storage:
  path: "netwatch.sqlite"

api:
  enabled: true

logging:
  level: "info"

discovery:
  enabled: true
  interval: 300

health:
  interval: 15

snmp:
  # NOT IMPLEMENTED — placeholder only; enabling has no effect
  enabled: false
  interval: 60

scheduler:
  concurrency: 10

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
```

Run with config:
```bash
./target/release/netwatch --config config.yaml
```

Or use environment variables:
```bash
NETWATCH_STORAGE_PATH=/data/netwatch.sqlite NETWATCH_LOG_LEVEL=debug ./target/release/netwatch
```

**Note:** The following config sections from earlier versions are no longer used and have been removed:
- `metrics.path` — Prometheus metrics use in-memory registry, not SQLite
- `ports` — Port scanning not yet implemented (Obj 6)
- `scheduler.rate_limit` — Only `concurrency` is used for job limiting