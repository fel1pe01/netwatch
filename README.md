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
| 7 | **Intelligent Anomaly Detection** | Medium-High | **Partial** (P4 - spike/drift detection, baselines, EventBus, scheduler, incident correlation implemented; unit tests, pattern break, detector registry pending) |
| 8 | **Professional Software Engineering** | Critical (transversal) | **Partial** (CI passes; CD, SBOM, signing missing) |
| 9 | **Distributed & Multi-site Monitoring** | High (long-term) | **Planned** |
| 10 | **Root Cause Analysis** | High | **Partial** (P4 - RootCauseAnalyzer, Hypothesis model, operator confirmation API implemented; needs DependsOn edges in topology) |
| 11 | **Change Audit & Drift Detection** | Medium-High | **Partial** (drift detection complete; expected inventory config, drift→incident correlation pending) |
| 12 | **Maintenance & Change Management** | Medium-High | **Partial** (silence fields exist; no silence engine, calendars) |
| 13 | **Synthetic Service Monitoring** | High | **Partial** (HTTP, DNS, TLS collectors implemented; scheduler integration pending) |
| 14 | **Capacity Planning** | Medium | **Partial** (P4 - linear regression, time-to-saturation, markdown/JSON reports, scheduler job implemented; CLI/API, unit tests, SNMP data, exponential fit, confidence intervals pending) |
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
| Test suite | ✅ 46 tests passing | Unit, integration, CLI, HTTP, metrics cardinality, drift |
| E2E smoke test | ✅ 42/42 passing | `scripts/e2e_smoke.sh` |

**Key limitations:** No authentication/TLS on API, Linux-only discovery, ICMP requires `ping` binary, no config hot-reload, SNMP collector placeholder, no pattern break detection, no detector registry, no CLI/API for capacity planning.

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

## License

MIT OR Apache-2.0