# Netwatch Architecture

This document describes the **current architecture** (as implemented) and the **target architecture** (to support the 16 strategic objectives). It separates verified implementation from design proposals.

---

## Current Architecture (Implemented)

### Module Map

```
src/
├── main.rs              # CLI entry point, HTTP server bootstrap, wiring
├── lib.rs               # Module declarations
├── config.rs            # YAML configuration structures (WIRED)
├── logging.rs           # Tracing subscriber init
├── api.rs               # Axum router + handlers
├── metrics.rs           # AppMetrics (Prometheus)
├── storage.rs           # SQLite pool + migrations (WIRED)
├── scheduler.rs         # Job queue + worker pool (RUNNING)
├── events.rs            # Broadcast EventBus (EMITTING events)
├── incidents.rs         # Incident state machine + persistence
├── drift.rs             # Drift detection (expected vs actual)
├── topology.rs          # Topology graph models
├── topology_builder.rs  # Topology construction from discovery/SNMP
├── topology_store.rs    # Topology persistence + snapshots
├── snmp.rs              # SNMP collector (placeholder)
├── monitoring/
│   ├── mod.rs           # HealthCheckResult, re-exports
│   ├── icmp.rs          # ping() via system `ping` binary
│   ├── tcp.rs           # check_tcp() via Tokio TcpStream
│   ├── dns.rs           # check_dns() via hickory-resolver
│   ├── http.rs          # check_http() with phase timing
│   ├── tls.rs           # check_tls() with cert validation
│   └── metrics.rs       # Health check Prometheus metrics (lazy)
├── discovery/
│   ├── mod.rs           # Re-exports
│   ├── interfaces.rs    # discover_interfaces() via if_addrs + /sys
│   └── routing.rs       # discover_routes() via /proc/net/route
```

### Component Responsibilities

| Module | Responsibility | Status |
|--------|----------------|--------|
| `main.rs` | CLI parsing, command dispatch, HTTP server startup, wiring all components | ✅ Working |
| `config.rs` | Typed YAML config with defaults, env overrides, `--config` flag | ✅ Wired |
| `api.rs` | HTTP routes, request/response, metrics side-effects, DB queries | ✅ Core + devices working |
| `metrics.rs` | AppMetrics (uptime, info, scheduler counters) | ✅ Working |
| `monitoring/*` | ICMP/TCP/DNS/HTTP/TLS checks, HealthCheckResult, Prometheus metrics | ✅ Working |
| `discovery/*` | Local interface & route discovery | ✅ Working (Linux-only) |
| `storage.rs` | SQLite connection, migrations, WAL + FK | ✅ Wired |
| `scheduler.rs` | Periodic job execution, concurrency control, graceful shutdown | ✅ Running |
| `events.rs` | In-memory broadcast event bus, typed events, emission helpers | ✅ Emitting |
| `incidents.rs` | Incident state machine, persistence, cooldown/silence fields | ✅ Implemented |
| `drift.rs` | Expected vs actual topology comparison, JSON/Markdown export | ✅ Complete |
| `topology.rs` | Graph models (nodes, edges, snapshots, diffs) | ✅ Implemented |
| `topology_builder.rs` | Build graph from discovery/SNMP, Mermaid/DOT/JSON export | ✅ Working |
| `topology_store.rs` | Persist topology nodes/edges, snapshots, diff with latest | ✅ Working |
| `snmp.rs` | SNMP collector framework | ⚠️ Placeholder job |

### Execution Flows

#### CLI ICMP Check
```
main()
  → Cli::parse()
  → match MonitorKind::Ping { target }
  → monitoring::ping(&target, 2000).await
  → tokio::process::Command::new("ping") -c 1 -w <sec> <target>
  → parse stdout regex `time=([\d.]+)`
  → HealthCheckResult { protocol: "icmp", ... }
  → HEALTH_CHECK_TOTAL.with_label_values(&["icmp"]).inc()
  → if !success: HEALTH_CHECK_FAILURES_TOTAL.inc()
  → if latency: HEALTH_CHECK_LATENCY_SECONDS.observe(latency_sec)
  → println!(JSON)
```

#### HTTP ICMP Check
```
POST /api/v1/monitor/ping
  → ping_handler(Json(req))
  → monitoring::ping(&req.target, req.timeout_ms).await
  → same as CLI → Json(res)
  → metrics recorded via same static counters
```

#### Scheduled Monitoring (Server Mode)
```
main()
  → Config loaded (file + env)
  → Storage::connect() → migrations run
  → TopologyStore + TopologyBuilder initialized
  → EventBus created
  → Scheduler instantiated (concurrency from config)
  → register_monitoring_jobs():
      - health_check_icmp (interval from config.health.interval) — target: 127.0.0.1 (hardcoded)
      - health_check_http (interval from config.health.interval) — target: http://127.0.0.1:8080/health (hardcoded)
      - health_check_dns (interval from config.health.interval) — target: localhost (hardcoded)
      - health_check_tls (interval from config.health.interval) — target: localhost:8080 (hardcoded)
      - topology_discovery (if config.discovery.enabled)
      - snmp_poll (if config.snmp.enabled, placeholder)
  → Each job:
      → runs check (ping/http/dns/tls)
      → records metrics
      → persists observation to SQLite
      → emits CheckCompleted/CheckFailed event via EventBus
  → Topology job:
      → build_from_discovery() → create_snapshot()
      → emits TopologyChanged events
  → Scheduler runs in background tokio task
  → Graceful shutdown on SIGINT
```

**Note:** Health check targets are currently hardcoded to localhost/127.0.0.1. Per-device/service scheduling requires inventory population (Objective 5) and is planned for P1+.

#### Metrics Exposition
```
GET /metrics
  → AppMetrics::metrics_handler(uptime, start_time)
  → Updates uptime gauge from Instant::elapsed()
  → prometheus::default_registry().gather()
  → TextEncoder::encode()
  → Returns: AppMetrics + health check metrics (if registered)
```

### Data Flow Diagram (Current)

```mermaid
graph TB
    subgraph CLI["CLI Entry Points"]
        CLI_PING["monitor ping"]
        CLI_TCP["monitor tcp"]
        CLI_DISC["discover local"]
        CLI_SERVER["server mode"]
    end

    subgraph HTTP["HTTP Server (Axum)"]
        API_ROUTER["api::router(storage)"]
        HEALTH["GET /health"]
        INTERFACES["GET /api/v1/interfaces"]
        ROUTES["GET /api/v1/routes"]
        DEVICES["GET /api/v1/devices (from DB)"]
        PING_API["POST /api/v1/monitor/ping"]
        TCP_API["POST /api/v1/monitor/tcp"]
        METRICS_EP["GET /metrics"]
        PLACEHOLDERS["Empty endpoints: vlans, subnets, gateways, events, topology"]
    end

    subgraph MONITORING["Monitoring Core"]
        PING_FN["monitoring::ping()"]
        TCP_FN["monitoring::check_tcp()"]
        DNS_FN["monitoring::check_dns()"]
        HTTP_FN["monitoring::check_http()"]
        TLS_FN["monitoring::check_tls()"]
        HC_RESULT["HealthCheckResult"]
        HC_METRICS["monitoring::metrics\nHEALTH_CHECK_*"]
    end

    subgraph DISCOVERY["Discovery"]
        IF_DISC["discovery::interfaces"]
        RT_DISC["discovery::routing"]
    end

    subgraph TOPOLOGY["Topology & Drift"]
        TOPO_BUILDER["topology_builder::TopologyBuilder"]
        TOPO_STORE["topology_store::TopologyStore"]
        DRIFT["drift::DriftDetector"]
        TOPO_EXPORTS["Mermaid / DOT / JSON"]
    end

    subgraph INFRA["Infrastructure (Wired)"]
        CONFIG["config::Config"]
        STORAGE["storage::Storage"]
        SCHEDULER["scheduler::Scheduler"]
        EVENTS["events::EventBus"]
        INCIDENTS["incidents::IncidentStore"]
    end

    subgraph METRICS_SYS["Metrics System"]
        APP_METRICS["metrics::AppMetrics"]
        DEFAULT_REG["prometheus::default_registry"]
    end

    CLI_PING --> PING_FN
    CLI_TCP --> TCP_FN
    CLI_DISC --> IF_DISC
    CLI_DISC --> RT_DISC
    CLI_SERVER --> API_ROUTER

    API_ROUTER --> HEALTH
    API_ROUTER --> INTERFACES
    API_ROUTER --> ROUTES
    API_ROUTER --> DEVICES
    API_ROUTER --> PING_API
    API_ROUTER --> TCP_API
    API_ROUTER --> METRICS_EP
    API_ROUTER --> PLACEHOLDERS

    PING_API --> PING_FN
    TCP_API --> TCP_FN
    INTERFACES --> IF_DISC
    ROUTES --> RT_DISC
    DEVICES --> STORAGE

    PING_FN --> HC_RESULT
    TCP_FN --> HC_RESULT
    DNS_FN --> HC_RESULT
    HTTP_FN --> HC_RESULT
    TLS_FN --> HC_RESULT
    PING_FN --> HC_METRICS
    TCP_FN --> HC_METRICS
    DNS_FN --> HC_METRICS
    HTTP_FN --> HC_METRICS
    TLS_FN --> HC_METRICS

    SCHEDULER --> PING_FN
    SCHEDULER --> HTTP_FN
    SCHEDULER --> DNS_FN
    SCHEDULER --> TLS_FN
    SCHEDULER --> TOPO_BUILDER

    TOPO_BUILDER --> IF_DISC
    TOPO_BUILDER --> RT_DISC
    TOPO_BUILDER --> TOPO_STORE
    TOPO_BUILDER --> EVENTS
    TOPO_BUILDER --> DRIFT
    TOPO_BUILDER --> TOPO_EXPORTS

    EVENTS -.->|CheckCompleted/Failed| INCIDENTS
    EVENTS -.->|TopologyChanged| INCIDENTS

    HC_RESULT --> STORAGE
    HC_RESULT --> EVENTS

    METRICS_EP --> APP_METRICS
    APP_METRICS --> DEFAULT_REG
    HC_METRICS --> DEFAULT_REG

    CONFIG --> SCHEDULER
    CONFIG --> STORAGE
    CONFIG --> TOPO_BUILDER
```

### Persistence Schema (Migrations)

`migrations/20231006000000_initial.sql` creates 11 tables:

| Table | Purpose | Key Columns |
|-------|---------|-------------|
| `sites` | Physical/logical locations | id, name |
| `networks` | CIDR blocks | id, cidr |
| `vlans` | VLAN definitions | id, number, name |
| `devices` | Discovered/inventoried devices | id, hostname, ip, mac, type, source, confidence |
| `interfaces` | Device interfaces | id, device_id (FK), name, mac, mtu, state |
| `services` | Monitored services/ports | id, device_id (FK), port, protocol, name, status |
| `events` | Incident/event log | id, device_id (FK), type, severity, timestamp, message |
| `topology_edges` | Graph relationships (legacy) | id, source_id, target_id, relationship, confidence, evidence |
| `observations` | Metric samples | id, device_id (FK), metric, value, source, confidence |

`migrations/20241009000000_topology.sql` adds topology tables:

| Table | Purpose |
|-------|---------|
| `topology_nodes` | Current topology nodes with properties, state |
| `topology_edges` | Current topology edges with provenance, confidence |
| `topology_snapshots` | Versioned topology snapshots (JSON) |

Indexes on: `devices(ip)`, `interfaces(name)`, `services(port)`, `events(timestamp)`, `topology_nodes(type)`, `topology_nodes(state)`, `topology_edges(source)`, `topology_edges(target)`, `topology_snapshots(timestamp)`.

**Current status:** Scheduler writes observations; API `/devices` queries devices table; topology builder persists nodes/edges/snapshots; drift detector compares expected vs actual.

### Metrics Catalog (Current)

| Metric | Type | Labels | Trigger |
|--------|------|--------|---------|
| `netwatch_info` | Gauge | — | Server start |
| `netwatch_uptime_seconds` | Gauge | — | Each `/metrics` scrape |
| `netwatch_scheduler_jobs_total` | Counter | — | Scheduler job start (via AppMetrics) |
| `netwatch_scheduler_errors_total` | Counter | — | Scheduler error (via AppMetrics) |
| `netwatch_health_checks_total` | Counter | `protocol` | Every ping/tcp/dns/http/tls check |
| `netwatch_health_check_failures_total` | Counter | `protocol` | Failed check |
| `netwatch_health_check_latency_seconds` | Histogram | `protocol` | Successful check with latency |

**Cardinality policy:** Only `protocol` (values: "icmp", "tcp", "dns", "http", "https", "tls") is a dynamic label. Enforced by tests.

---

## Target Architecture (Design)

### High-Level Modules

```
netwatch/
├── config/              # Typed config, validation, hot-reload, precedence
├── domain/              # Core models: Device, Service, Check, Incident, Topology, SLO
├── storage/             # Repositories, migrations, transactions, retention
├── discovery/           # Incremental discovery, topology builder, diff engine
├── monitoring/          # Collectors: ICMP, TCP, DNS, HTTP, SNMP, Synthetic
├── scheduler/           # Cron-like scheduler, jitter, backoff, graceful shutdown
├── history/             # Aggregations, percentiles, SLO computations, retention
├── events/              # Typed event contracts, versioning, emission
├── incidents/           # State machine, deduplication, correlation, ACK, resolve
├── notifications/       # Adapters: Telegram, Webhook, Alertmanager (pluggable)
├── api/                 # REST v1, OpenAPI, auth, validation, pagination
├── metrics/             # Prometheus exporter, cardinality guard, custom registry
├── topology/            # Graph, dependency analysis, export (Mermaid/DOT/JSON)
├── security/            # Secrets, RBAC, SSRF protection, rate limits
├── observability/       # Structured logs, traces, diagnostics, profiling
└── cli/                 # Admin, one-shot checks, config validate, diag
```

### Module Boundaries & Interfaces

```
┌─────────────────────────────────────────────────────────────────┐
│                        EXTERNAL INTEGRATIONS                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐         │
│  │ Prometheus│  │ Grafana  │  │ Alertmgr │  │ Telegram │         │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘         │
└───────┼─────────────┼─────────────┼─────────────┼────────────────┘
        │             │             │             │
        ▼             ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────────┐
│                         ADAPTER LAYER                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
│  │MetricsExporter│  │DashboardProv │  │AlertRouter   │           │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘           │
└─────────┼─────────────────┼─────────────────┼────────────────────┘
          │                 │                 │
          ▼                 ▼                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                         CORE DOMAIN                              │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐           │
│  │ Scheduler │ │ Monitoring│ │ Discovery │ │ Incidents │           │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘           │
└───────┼─────────────┼─────────────┼─────────────┼────────────────┘
        │             │             │             │
        ▼             ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────────┐
│                       PERSISTENCE LAYER                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
│  │ SQLite (primary)│  │ Kafka (opt)  │  │ Object Store │          │
│  └──────────────┘  └──────────────┘  └──────────────┘           │
└─────────────────────────────────────────────────────────────────┘
```

### Key Interfaces (Traits for Swappable Adapters)

```rust
// Monitoring collectors
#[async_trait]
trait Collector: Send + Sync {
    type Config;
    type Result;
    async fn check(&self, target: &Target, cfg: &Self::Config) -> Result<Self::Result>;
    fn protocol(&self) -> &'static str;
}

// Event delivery
#[async_trait]
trait EventSink: Send + Sync {
    async fn emit(&self, event: Event) -> Result<()>;
    fn name(&self) -> &'static str;
}

// Notification delivery
#[async_trait]
trait Notifier: Send + Sync {
    async fn notify(&self, incident: &Incident, event: &IncidentEvent) -> Result<()>;
}

// Topology export
trait TopologyExporter: Send + Sync {
    fn export_mermaid(&self, graph: &TopologyGraph) -> String;
    fn export_dot(&self, graph: &TopologyGraph) -> String;
    fn export_json(&self, graph: &TopologyGraph) -> serde_json::Value;
}
```

### Data Flows (Target)

#### Scheduled Monitoring
```
Scheduler (tick)
  → for each (device, service) in registry:
      → spawn check task (semaphore-limited)
      → Collector::check() → CheckResult
      → HistoryRepository::record(result)
      → Events::emit(CheckCompleted { result })
      → if threshold breached: Events::emit(ThresholdBreached { ... })
```

#### Incident Lifecycle
```
ThresholdBreached / CheckFailed / TopologyChange
  → IncidentEngine::process(event)
  → dedup_key + cooldown check
  → correlation: find related open incidents (shared gateway/subnet)
  → create Incident { state: Open, evidence: [...] }
  → Events::emit(IncidentOpened { incident })
  → Notifier::notify(incident, IncidentEvent::Opened)
  → ...
  → CheckRecovered → IncidentEngine::resolve(incident)
  → Events::emit(IncidentResolved { incident })
  → Notifier::notify(incident, IncidentEvent::Resolved)
```

#### Topology Build
```
DiscoveryScheduler (interval)
  → Discovery::run() → DiscoveredData { interfaces, routes, neighbors, snmp }
  → TopologyBuilder::merge(current, discovered)
  → for each change: Events::emit(TopologyChanged { diff })
  → TopologyRepository::persist(graph)
```

### Persistence Strategy

| Data Type | Primary Store | Retention | Notes |
|-----------|---------------|-----------|-------|
| Inventory (devices, interfaces, services) | SQLite | Forever | Source of truth |
| Check observations (raw) | SQLite | 7d (configurable) | High volume; batch inserts |
| Aggregated metrics (hourly/daily) | SQLite | 13mo | Materialized views |
| Events/Incidents | SQLite | 3y | Audit trail |
| Topology snapshots | SQLite | 90d | Diff baseline |
| Prometheus metrics | Prometheus TSDB | 30d (config) | Operational dashboards |
| Kafka events | Kafka | 7d (config) | Audit/stream processing |

**Transactional outbox pattern** for SQLite → Kafka: write event to `outbox` table in same transaction as domain change; Kafka publisher reads outbox.

### Security Model

| Layer | Controls |
|-------|----------|
| API | Optional auth (Bearer token / mTLS), rate limiting, body size limits, CORS |
| Config | Secrets via env/file only; never in YAML; redacted in logs |
| Collectors | Allowlist targets (from inventory); no arbitrary host/port from API |
| Containers | Non-root, read-only rootfs, dropped capabilities |
| Network | Internal Docker network; only API port exposed; Kafka/Prometheus internal |
| Dependencies | `cargo audit` in CI; `cargo deny` for licenses; pinned versions |

### Configuration Precedence

1. Compile-time defaults (`Config::default()`)
2. Config file (`--config` or `$HOME/.config/netwatch/netwatch.yaml`)
3. Environment variables (`NETWATCH_*` prefix)
4. CLI flags (for server: `--listen`, `--log-level`)

Hot-reloadable: log level, scheduler intervals, check timeouts, collector config.
Requires restart: listen address, storage path, TLS certs.

### Error Handling Strategy

- **Expected failures** (target down, timeout): `CheckResult { success: false, error: Some(...) }` — not an `Err`
- **Infrastructure failures** (DB down, Kafka down): `Result::Err` → logged, metrics incremented, scheduler pauses affected jobs
- **Config errors**: Fail fast at startup with actionable message
- **Unknown states**: Modeled explicitly (e.g., `CheckStatus::Unknown`, `DeviceState::Unknown`)

### Observability Built-In

- **Logs**: Structured JSON (tracing), correlation IDs propagated
- **Metrics**: Prometheus exposition, cardinality-guarded, exemplars for traces
- **Traces**: OpenTelemetry-ready (future), span per check/incident
- **Diagnostics**: `/debug/vars`, `/debug/pprof` (dev only), config dump (sanitized)

---

## Evolution Path (Current → Target)

| Step | Action | Unlocks |
|------|--------|---------|
| 1 | ~~Wire `Config` to `main.rs` (`--config`, env override)~~ **DONE** | P0 foundation |
| 2 | ~~Initialize `Storage` at startup, pass to API handlers~~ **DONE** | Persistent inventory |
| 3 | ~~Instantiate `Scheduler`, register monitoring jobs~~ **DONE** | Periodic checks |
| 4 | ~~Persist `CheckResult` to `observations` table~~ **DONE** | Historical data |
| 5 | ~~Implement `EventBus` emitters in monitoring/scheduler~~ **DONE** | Event-driven core |
| 6 | Build `IncidentEngine` consuming events | Alerting |
| 7 | Add Telegram notifier + Alertmanager integration | Notifications |
| 8 | Dockerfile + compose profiles | Deployable stack |
| 9 | Prometheus rules + Grafana dashboards | Observability |
| 10 | SNMP collector + inventory enrichment | Infrastructure visibility |
| 11 | ~~Topology graph builder + exports~~ **DONE** | Live map |
| 12 | SLA/SLO calculator + error budget alerts | Objective 1 |
| 13 | Maintenance/silence engine | Objective 12 |
| 14 | ~~Change audit + drift detection~~ **DONE** | Objective 11 |
| 15 | Incident report generator | Objective 15 |
| 16 | Backup/restore scripts + CI drill | Objective 16 |
| 17 | ~~Synthetic collectors (HTTP, DNS, TLS)~~ **DONE** | Objective 13 |
| 18 | ~~Anomaly detectors + baselines~~ **DONE** | Objective 7 |
| 19 | Root cause hypotheses | Objective 10 |
| 20 | Distributed agent protocol | Objective 9 |
| 21 | Traffic analysis (NetFlow/sFlow) | Objective 6 |

---

## Decisions & Trade-offs

| Decision | Rationale | Revisit When |
|----------|-----------|--------------|
| Monolithic binary (not microservices) | Simpler deployment, debugging, testing; split only when scaling demands | Team >5, distinct scaling profiles |
| SQLite for primary persistence | Zero-ops, embedded, ACID, WAL supports concurrent readers | Write throughput >10k ops/s |
| `prometheus::default_registry()` | Simplicity; shared with health check metrics | Custom registry needed for multi-tenant |
| System `ping` for ICMP | No CAP_NET_RAW needed; works in containers with `--cap-add=NET_RAW` | Raw socket implementation for portability |
| Linux-only discovery (`/sys`, `/proc`) | MVP scope; cross-platform later via `rtnetlink`/`getifaddrs` | Windows/macOS agent requirement |
| Broadcast channel for events | In-process, zero-dep, sufficient for single-node | Multi-node → Kafka/NATS |
| No auth in MVP | Internal tool assumption; add when exposing externally | Public/network exposure |

---

## Limitations (Current)

1. **No authentication/TLS** — All endpoints open, HTTP only
2. **No config hot-reload** — Restart required for any change
3. **Linux-only discovery** — `/sys/class/net`, `/proc/net/route`
4. **ICMP requires `ping` binary** — No raw socket fallback
5. **SNMP collector placeholder** — Job exists but no real polling
6. **Placeholders for 5 API endpoints** — `/vlans`, `/subnets`, `/gateways`, `/events`, `/topology`
7. **No SLA/SLO calculations** — Availability/percentiles not computed from observations
8. **No incident engine consuming events** — EventBus emits but no subscriber creates incidents
9. **No Telegram/Alertmanager notifiers** — Alerting not implemented
10. **Stack deployment untested** — Docker Compose profiles exist but not validated in CI

---

## References

- [Implementation Status](IMPLEMENTATION_STATUS.md) — Per-objective verification
- [Roadmap](ROADMAP.md) — Phases, acceptance criteria, risks
- [Architecture Audit](ARCHITECTURE_AND_FUNCTIONAL_TESTS.md) — Full code-backed audit
- [Development Guide](DEVELOPMENT.md) — Build/test commands