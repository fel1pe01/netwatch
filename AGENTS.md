# Netwatch — Agent Guide

Rust network monitoring agent (Tokio + Axum + SQLite). Linux/WSL target. Discovers interfaces/routes, runs ICMP/TCP/DNS/HTTP/TLS checks, SNMP placeholder, topology graph, drift detection, anomaly detection, capacity planning, alerting, incident correlation, Prometheus metrics, REST API.

---

## Developer Commands

Run these yourself — don't ask the user.

| Task | Command |
|------|---------|
| Clean build | `cargo clean` |
| Format check | `cargo fmt --check` |
| Lint (strict) | `cargo clippy --all-targets --all-features -- -D warnings` |
| Unit + integration tests | `cargo test --locked --all-targets --all-features --no-fail-fast` |
| Doc tests | `cargo test --locked --doc --all-features` |
| Release build | `cargo build --locked --release` |
| E2E smoke test | `./scripts/e2e_smoke.sh` (needs release binary) |
| **Full pre-push validation** | `./scripts/check.sh` |
| Quick iteration | `./scripts/check.sh --quick` |
| Skip Docker (WSL w/o Docker Desktop) | `./scripts/check.sh --skip-docker` |
| Skip security tools | `./scripts/check.sh --skip-audit` |
| Skip E2E | `./scripts/check.sh --skip-e2e` |

**A task is not done until `fmt`, `clippy`, `test`, and `build --release` all pass.**

---

## Project Structure

```
src/
  main.rs              # CLI entry, server setup, job registration
  lib.rs               # Re-exports
  config.rs            # Config structs, env var overrides, YAML loading
  logging.rs           # tracing init
  storage.rs           # SQLite + sqlx migrations
  api.rs               # Axum router, REST endpoints
  metrics.rs           # Prometheus metrics (AppMetrics + health check metrics)
  scheduler.rs         # Async job scheduler with concurrency limit
  events.rs            # EventBus (broadcast), CheckEvidence, EntityType
  incidents.rs         # IncidentEngine, IncidentStore, state machine
  drift.rs             # DriftDetector (expected vs actual)
  topology.rs/.builder.rs/.store.rs  # Graph + Mermaid/DOT/JSON export
  snmp.rs              # SNMP collector (placeholder)
  capacity_planning.rs # Linear regression, time-to-saturation, reports
  anomaly.rs           # Spike/drift/pattern-break detection, baselines
  alerts.rs            # AlertEngine: 6 rule types, cooldown, dedup
  notifications.rs     # TelegramNotifier (rate limited, retry/backoff)
  ssrf.rs              # SSRF protection (CIDR/hostname allow/block lists)
  rate_limit.rs        # IP-based rate limiter (dashmap)
  auth.rs              # Token auth (optional), public paths
  tls.rs               # TLS config loading (rustls)
  monitoring/          # ICMP, TCP, DNS, HTTP, TLS check implementations
  discovery/           # Linux /sys/class/net + /proc/net/route parsing
tests/                 # Integration tests (93 tests)
scripts/
  check.sh             # Full CI replica (fmt, check, test, doc-test, clippy, release, audit, deny, e2e, docker)
  e2e_smoke.sh         # 42 checks against release binary (CLI, HTTP, metrics, topology)
  known-failures.txt   # Allowlisted flaky tests
migrations/            # SQL (append-only, never edit applied migrations)
docs/                  # Architecture, roadmap, implementation status, testing
prometheus/            # prometheus.yml + rules
grafana/               # Provisioning + dashboards
alertmanager/          # Alertmanager config
compose.yaml           # Full stack: netwatch, prometheus, grafana, alertmanager, kafka (profiles)
Dockerfile             # Multi-stage, non-root, healthcheck
```

---

## Code Rules

- **No `unwrap()`/`expect()` outside tests** — use `thiserror` (library) / `anyhow` (binary).
- **Logs via `tracing`** — never `println!` except CLI command output.
- **Metrics cardinality bounded** — only `protocol` label on health check metrics; no IPs, hosts, targets as labels.
- **SSRF protection** — all outbound HTTP/DNS/TCP/TLS go through `SsrfPolicy`; config in `ssrf:` section.
- **New dependencies** — justify and verify not redundant; `cargo deny` enforces bans/licenses.
- **Public API / config format changes** — update docs (`docs/*.md`, `README.md`) and tests.
- **All network/SNMP/config input is untrusted** — validate, limit sizes, enforce timeouts.
- **Migrations are append-only** — never edit files already applied.

---

## Workflow

1. **Start**: run `cargo clean`. Repeat if build/test results look suspicious.
2. **Read before editing** — `rg` to verify assumptions.
3. **Small, verifiable changes** — one logical topic per commit.
4. **Add/update tests** with every behavioral change.
5. **Update docs** (`docs/*.md`, `README.md`, `AUDIT_FINAL_REPORT.md`) if real state changes.
6. **Never mark "done"** in docs unless connected, tested, and verified.
7. **Pre-push**: run `./scripts/check.sh` (or `--skip-docker` on WSL without Docker Desktop).

---

## Configuration

- File: `config.yaml` (or `netwatch.yaml` in `$XDG_CONFIG_HOME/netwatch/`)
- Example: `config.example.yaml`
- Env overrides: `NETWATCH_STORAGE_PATH`, `NETWATCH_LOG_LEVEL`, `NETWATCH_LISTEN`
- `${VAR:-default}` expansion in YAML
- **Secrets via env vars** — never hardcode in config (see `config.example.yaml` for Telegram, SNMPv3)

Key sections:
- `agent.listen` — HTTP bind address
- `storage.path` — SQLite file
- `discovery.enabled/interval` — interface/route discovery
- `health.interval` — legacy ICMP check interval
- `synthetic_checks` — HTTP/HTTPS/DNS/TCP/TLS checks with per-phase latency metrics
- `scheduler.concurrency` — max concurrent jobs
- `anomaly` — detector config (spike, drift, pattern break)
- `alert` — rules (Down, Up, LatencyThreshold, LatencyPercentile, FailureRate, SustainedDegradation)
- `capacity` — linear regression, reports
- `ssrf` — CIDR/hostname allow/block, DNS rebinding protection
- `rate_limit` — IP-based, excluded paths
- `auth` — optional token auth, public paths
- `tls` — HTTPS server cert/key
- `snmp` — **placeholder only** (tables exist, no collection code)

---

## Testing

- **Unit/integration**: `cargo test` — 143 tests (config, storage, scheduler, discovery, monitoring, HTTP, metrics cardinality, drift, anomaly, capacity, incidents, root cause, notifications, CLI, default config)
- **E2E smoke**: `./scripts/e2e_smoke.sh` — 40 checks against release binary (5 CLI, 12 HTTP, 10 metrics, 6 placeholder endpoints, 1 invalid content-type, 6 metrics)
- **Known failures**: listed in `scripts/known-failures.txt` (basename only, one per line, `#` comments)
- **CI parity**: `scripts/check.sh` mirrors `.github/workflows/ci.yml` (if present); keep in sync
- **Test DB isolation**: each test uses `tempfile::tempdir()` for SQLite

---

## Docker / Compose

- **Dockerfile**: multi-stage, `rust:1.99.0-bookworm` builder → `debian:bookworm-slim` runtime, non-root user, healthcheck
- **Image tag for validation**: `netwatch:check` (built fresh each `check.sh` run)
- **Compose profiles**: `core`, `observability`, `messaging`, `full`
- **WSL2 + Docker Desktop**: enable WSL Integration in Docker Desktop Settings → Resources → WSL Integration
- **Skip Docker in CI replica**: `./scripts/check.sh --skip-docker`

---

## Key Implementation Notes

| Area | Status | Notes |
|------|--------|-------|
| ICMP monitoring | ✅ | System `ping` binary; Linux only; no raw socket fallback |
| TCP/DNS/HTTP/TLS | ✅ | Full per-phase latency (DNS, TCP, TLS, TTFB) |
| SNMP | ⚠️ | Tables + config exist; `SnmpCollector` is placeholder |
| Discovery | ✅ | Linux `/sys/class/net` + `/proc/net/route` |
| Topology | ✅ | Graph from discovery; Mermaid/DOT/JSON export; `DependsOn`/`GatewayOf` edges for incident correlation |
| Scheduler | ✅ | Jobs for ICMP, HTTP, DNS, TLS, synthetic, topology, SNMP, anomaly, capacity, alerts, service checks |
| EventBus | ✅ | Broadcast (1000); topology changes, check results, anomaly events, alert events |
| Alert engine | ✅ | 6 rule types, cooldown, dedup, emits `ThresholdBreached`/`ThresholdRecovered` |
| Notifications | ✅ | Telegram (HTML, rate limit, exponential backoff retry) |
| Incident engine | ✅ | State machine, auto-resolve, correlation via topology gateway edges |
| Drift detection | ✅ | Expected vs actual; JSON/Markdown export |
| Anomaly detection | ✅ | Spike, drift, pattern break; baselines; event emission; false positive tracking pending |
| Capacity planning | ✅ | Linear regression, time-to-saturation, markdown/JSON reports; confidence intervals pending |
| Auth on API | ✅ | Token-based (Bearer), roles (Admin/Operator/Viewer), public paths `/health`, `/metrics` |
| TLS on API | ❌ | **NOT implemented** — server refuses to start if TLS configured (fail-closed) |
| Config hot-reload | ❌ | Not implemented |

---

## Common Gotchas

- **Port 8080 conflict**: E2E fails if another netwatch is running. Stop it first.
- **Metrics cardinality tests** fail if you add labels like `host`, `target`, `ip` to health check metrics.
- **SSRF blocks private IPs by default** — synthetic checks to `10.x`, `192.168.x`, `127.0.0.1` need `allow_loopback: true` or `allowed_cidrs` in config.
- **ICMP needs `ping` binary** — present in `iputils-ping` (Dockerfile installs it).
- **SQLite WAL mode** — `netwatch.sqlite-wal`/`-shm` files appear; don't commit them.
- **`cargo clean` before `check.sh`** — required for true CI parity (clean build from scratch).
- **`known-failures.txt`** — only suppresses "known" label in summary; tests still run and fail.
- **Toolchain drift**: CI may use newer `stable` than local; pin `RUST_TOOLCHAIN` in CI if needed.
- **Default bind is 0.0.0.0:8080** — binds to all interfaces; change to 127.0.0.1 in production or use firewall.

---

## References

- `README.md` — quick start, status table, security advisories
- `docs/ARCHITECTURE.md` — module boundaries, data flows
- `docs/ROADMAP.md` — 16 objectives, phases, acceptance criteria
- `docs/IMPLEMENTATION_STATUS.md` — per-objective verification with code refs
- `docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md` — full code-backed audit
- `docs/DEVELOPMENT.md` — this guide in long form
- `docs/TESTING.md` — test organization, fixtures
- `docs/OPERATIONS.md` — deployment, backup/restore, monitoring
- `docs/CONFIGURATION.md` — config reference
- `AUDIT_FINAL_REPORT.md` — latest audit summary

---

## ⚠️ Estado actual (2026-11-17)

**BUILD VERIFICADO** — 155 tests pass, 40/40 E2E checks pass, build clean.

| Component | Status |
|-----------|--------|
| TLS on API | ❌ **NOT implemented** — server refuses to start if TLS configured (fail-closed) |
| SNMP collector | ⚠️ **Placeholder** — tables exist, no polling code |
| Scheduler → checks → SQLite → events | ✅ **Complete** — jobs run with hysteresis (down after N failures, up after M successes from confirmed down); persist to check_results/events_v2 via CheckResultWriter actor; load last state from DB on startup; REST endpoints /api/v1/checks, /api/v1/events with pagination and filters; retention pruning job |
| Auth on API | ✅ Token-based (Bearer), roles, public paths `/health`, `/metrics` |
| Default bind | `127.0.0.1:8080` — secure by default |
| /health, /metrics | Public (no auth required) — by design |

Run `./scripts/check.sh` to verify before push.