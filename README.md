# Netwatch

Netwatch is a modular network monitoring agent written in Rust. It provides a command-line interface, an HTTP API, network discovery, ICMP/TCP health checks, and Prometheus metrics.

## Mission

Build an open-source, production-grade network observability platform that combines continuous monitoring, topology discovery, incident management, and SLA/SLO tracking — deployable as a single binary or container stack with reproducible configuration, persistent storage, and professional-grade engineering practices.

## Strategic Objectives

Netwatch is organized around 16 strategic objectives that define its product scope:

| # | Objective | Priority | Status |
|---|-----------|----------|--------|
| 1 | **Continuous Monitoring & SLA/SLO** | High | Planned |
| 2 | **Live Topology Map** | High | Planned |
| 3 | **Event & Incident Engine** | High | Planned |
| 4 | **Deployable Observability Stack** | High | Planned |
| 5 | **Inventory & SNMP Monitoring** | High | Planned |
| 6 | **Traffic Analysis & Consumption** | Medium-High | Planned |
| 7 | **Intelligent Anomaly Detection** | Medium-High | Planned |
| 8 | **Professional Software Engineering** | Critical (transversal) | Partial |
| 9 | **Distributed & Multi-site Monitoring** | High (long-term) | Planned |
| 10 | **Root Cause Analysis** | High | Planned |
| 11 | **Change Audit & Drift Detection** | Medium-High | Planned |
| 12 | **Maintenance & Change Management** | Medium-High | Planned |
| 13 | **Synthetic Service Monitoring** | High | Planned |
| 14 | **Capacity Planning** | Medium | Planned |
| 15 | **Reproducible Incident Reports** | Medium | Planned |
| 16 | **Verified Disaster Recovery** | High | Planned |

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
| HTTP API (inventory/topology) | ⚠️ Placeholders | Return empty arrays; not wired to storage |
| Prometheus metrics | ✅ Implemented | AppMetrics + health checks; `protocol` label only (bounded cardinality) |
| Configuration (YAML) | ⚠️ Defined, not wired | `Config` struct complete; server hardcodes 0.0.0.0:8080 |
| SQLite storage & migrations | ⚠️ Defined, not wired | 11 tables with indexes; `Storage::connect()` works but unused |
| Scheduler | ⚠️ Defined, not used | Job queue, semaphore pool, graceful shutdown — never instantiated |
| Event bus | ⚠️ Defined, not used | Broadcast channel — no emitters/subscribers |
| ICMP monitoring | ✅ Implemented | Uses system `ping`; Linux-only; no raw socket fallback |
| TCP monitoring | ✅ Implemented | Tokio `TcpStream` with timeout |
| Discovery (interfaces/routes) | ✅ Implemented | Linux `/sys/class/net` + `/proc/net/route` |
| Test suite | ✅ 41 tests passing | Unit, integration, CLI, HTTP, metrics cardinality |
| E2E smoke test | ✅ 42/42 passing | `scripts/e2e_smoke.sh` |

**Key limitations:** No config file support (`--config`), no persistence of check results, no authentication/TLS, no graceful shutdown, Linux-only discovery, scheduler/event bus/storage not integrated.

See [docs/IMPLEMENTATION_STATUS.md](docs/IMPLEMENTATION_STATUS.md) for detailed per-objective status with code references.

## Quick Start

```bash
# Build
cargo build --release

# Run server (hardcoded 0.0.0.0:8080)
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