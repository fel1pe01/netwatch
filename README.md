# Netwatch

Netwatch is a modular network monitoring agent written in Rust. It provides a command-line interface, an HTTP API, network discovery, ICMP/TCP/DNS/HTTP/TLS health checks, topology discovery, drift detection, scheduled monitoring with persistence, and Prometheus metrics.

## Mission

Build an open-source, production-grade network observability platform that combines continuous monitoring, topology discovery, incident management, and SLA/SLO tracking — deployable as a single binary or container stack with reproducible configuration, persistent storage, and professional-grade engineering practices.

## Strategic Objectives

Netwatch is organized around 16 strategic objectives that define its product scope:

| # | Objective | Priority | Status |
|---|-----------|----------|--------|
| 1 | **Continuous Monitoring & SLA/SLO** | High | **Partial** (P0 foundation complete; SLO math pending) |
| 2 | **Live Topology Map** | High | **Partial** (topology graph built from discovery; exports implemented) |
| 3 | **Event & Incident Engine** | High | **Partial** (typed events, incident state machine, persistence; alert rules, correlation, notifiers pending) |
| 4 | **Deployable Observability Stack** | High | **Partial** (Dockerfile, compose.yaml, Prometheus/Grafana/Alertmanager configs; stack deployment untested) |
| 5 | **Inventory & SNMP Monitoring** | High | **Planned** (tables exist; no SNMP code) |
| 6 | **Traffic Analysis & Consumption** | Medium-High | **Planned** |
| 7 | **Intelligent Anomaly Detection** | Medium-High | **Partial** (P4 - spike/drift/pattern break detection, baselines, EventBus, scheduler, incident correlation, deduplication implemented; unit tests, detector registry, false positive tracking pending) |
| 8 | **Professional Software Engineering** | Critical (transversal) | **Partial** (CI passes; CD, SBOM, signing missing) |
| 9 | **Distributed & Multi-site Monitoring** | High (long-term) | **Planned** |
| 10 | **Root Cause Analysis** | High | **Partial** (P4 - RootCauseAnalyzer, Hypothesis model, operator confirmation API implemented; needs DependsOn edges in topology) |
| 11 | **Change Audit & Drift Detection** | Medium-High | **Partial** (drift detection complete; expected inventory config, drift→incident correlation pending) |
| 12 | **Maintenance & Change Management** | Medium-High | **Partial** (silence fields exist; no silence engine, calendars) |
| 13 | **Synthetic Service Monitoring** | High | **Partial** (HTTP, DNS, TLS collectors implemented; scheduler integration pending) |
| 14 | **Capacity Planning** | Medium | **Partial** (P4 - linear regression with timestamps, time-to-saturation, markdown/JSON reports, scheduler job, CLI/API exposed; unit tests, SNMP data, confidence intervals, exponential fit pending) |
| 15 | **Reproducible Incident Reports** | Medium | **Planned** |
| 16 | **Verified Disaster Recovery** | High | **Planned** |

See [docs/ROADMAP.md](docs/ROADMAP.md) for detailed phases, dependencies, and acceptance criteria.

## Design Principles

- **Measurements first**: Correct, persisted, reproducible metrics — no placeholder data
- **Evidence-based alerts**: Every alert carries provenance, not just thresholds
- **Signal over noise**: Deduplication, correlation, and cooldowns built in
- **Verified recovery**: Backup/restore tested, not documented
- **Security by default**: No secrets in config, least privilege, no SSRF
- **Reproducible deployments**: Versioned config, pinned dependencies, SBOM
- **Modular architecture**: Clear boundaries, swappable adapters, no framework lock-in
- **Explicit error handling**: Unknown states modeled, not ignored
- **Honest documentation**: Implemented vs. planned clearly separated

## Current Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| CLI (`monitor ping/tcp`, `discover local`) | ✅ Implemented | JSON output, tested |
| HTTP API (core endpoints) | ✅ Implemented | `/health`, `/metrics`, `/interfaces`, `/routes`, `/monitor/ping`, `/monitor/tcp` |
| HTTP API (inventory/topology) | ⚠️ Partial | `/devices` queries DB; 5 endpoints remain placeholders |
| Prometheus metrics | ✅ Implemented | AppMetrics + health checks; `protocol` label only (bounded cardinality) |
| Configuration (YAML) | ✅ Implemented | `Config` struct complete; `--config` flag wired; env overrides work |
| SQLite storage & migrations | ✅ Implemented | 13 tables with indexes; `Storage::connect()` wired in server |
| Scheduler | ✅ Implemented | Instantiated in `main.rs`; jobs for ICMP, HTTP, DNS, TLS, topology, SNMP; graceful shutdown |
| Event bus | ✅ Implemented | Broadcast channel (1000); topology changes emit events; check events emitted |
| ICMP monitoring | ✅ Implemented | Uses system `ping`; Linux-only; no raw socket fallback |
| TCP monitoring | ✅ Implemented | Tokio `TcpStream` with timeout |
| DNS monitoring | ✅ Implemented | Hickory resolver; A/AAAA/CNAME/MX/TXT/NS |
| HTTP monitoring | ✅ Implemented | Status, body match, headers, redirects, per-phase timing |
| TLS monitoring | ✅ Implemented | Cert expiry, chain, hostname verification, OCSP |
| Discovery (interfaces/routes) | ✅ Implemented | Linux `/sys/class/net` + `/proc/net/route` |
| Topology builder | ✅ Implemented | Graph from discovery + SNMP; Mermaid/DOT/JSON export |
| Drift detection | ✅ Implemented | Expected vs actual comparison; JSON/Markdown export |
| Test suite | ⚠️ 46 tests passing (historical) | Unit, integration, CLI, HTTP, metrics cardinality, drift — **not re-verified since 2026-10-13** (blocked by toolchain: missing `dlltool.exe` in GNU target) |
| E2E smoke test | ⚠️ 42/42 passing (historical) | `scripts/e2e_smoke.sh` — **not re-verified since 2026-10-13** (requires release binary; blocked by toolchain) |

**Key limitations:** No authentication/TLS on API, Linux-only discovery, ICMP requires `ping` binary, no config hot-reload, SNMP collector placeholder, no detector registry, no false positive tracking, no confidence intervals for capacity projections.

See [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) for detailed per-objective status with code references.

## Quick Start

```bash
# Build
cargo build --release

# Run server (configurable via --config or env)
./target/release/netwatch

# CLI ad-hoc checks
./target/release/netwatch monitor ping 1.2.3.4
./target/release/netwatch monitor tcp example.com 443
./target/release/netwatch discover local

# API examples
curl http://localhost:8080/health
curl http://localhost:8080/api/v1/interfaces
curl -X POST http://localhost:8080/api/v1/monitor/ping -H "Content-Type: application/json" -d '{"target":"1.2.3.4"}'
curl http://localhost:8080/metrics
```

## Security & Dependency Status

All known vulnerabilities have been addressed as of 2026-10-13:

| Advisory | Dependency | Resolution |
|----------|------------|------------|
| RUSTSEC-2024-0363 | sqlx 0.7.4 → 0.8.6 | Updated; mysql/postgres features disabled via `default-features = false` |
| RUSTSEC-2024-0437 | protobuf 2.28.0 → 3.7.2 | Updated via prometheus 0.14.0 |
| RUSTSEC-2024-0436 | paste 1.0.15 | Removed (no longer transitive) |
| RUSTSEC-2025-0134 | rustls-pemfile 2.2.0 | Removed via rustls-native-certs 0.8.5 |
| RUSTSEC-2024-0449 | unsafe-libyaml (via serde_yaml) | Replaced with maintained `yaml_serde` 0.10.7 |
| RUSTSEC-2023-0071 | rsa 0.9.10 (via sqlx-mysql) | **Exception** — sqlx-mysql not enabled (mysql feature disabled) |

Validation: `cargo audit` passes with one documented exception; `cargo deny` passes.

## Documentation

- [Roadmap & Phases](docs/ROADMAP.md) — 16 objectives, priorities, dependencies, acceptance criteria
- [Architecture](docs/ARCHITECTURE.md) — Current vs. target architecture, module boundaries, data flows
- [Implementation Status](docs/IMPLEMENTATION_STATUS.md) — Per-objective verification with code/test evidence
- [Architecture Audit](docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md) — Full code-backed functional audit
- [Development Guide](docs/DEVELOPMENT.md) — Build, test, contribute
- [Changes Log](docs/CHANGES_MADE.md) — Implementation session change log

## Testing

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
./scripts/e2e_smoke.sh
```

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

## License

MIT OR Apache-2.0