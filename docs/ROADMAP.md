# Netwatch Roadmap

This document maps the 16 strategic objectives to implementation phases, dependencies, and verifiable acceptance criteria.

## Phase Overview

| Phase | Theme | Objectives | Status | Target |
|-------|-------|------------|--------|--------|
| P0 | Foundation | 8, 1 | ✅ Complete | Core wiring: config, storage, scheduler, basic monitoring persistence |
| P1 | Operational Observability | 3, 4, 13 | 🔄 In Progress | Events, incidents, alerts, Prometheus/Grafana stack, synthetic checks |
| P2 | Infrastructure Visibility | 2, 5 | 🔄 In Progress | Live topology, SNMP, inventory |
| P3 | Operational Reliability | 11, 12, 15, 16 | 🔄 In Progress | Drift detection ✅ Complete; Maintenance windows 🔄; Incident reports ⏳; Disaster recovery ⏳ |
| P4 | Intelligence & Capacity | 7, 10, 14 | 🔄 In Progress | Anomaly detection (Partial), root cause (Planned), capacity planning (Partial) |
| P5 | Scale & Advanced Analysis | 9, 6 | Planned | Distributed agents, traffic analysis |

**Objective 8 (Engineering Quality) is transversal** — CI/CD, testing, security, documentation apply to every phase.

### Phase Status Summary
- **P0**: ✅ Complete — Core wiring complete (config, storage, scheduler, monitoring persistence)
- **P1**: 🔄 In Progress — Events, incidents, alerts, Prometheus/Grafana stack, synthetic checks
- **P2**: 🔄 In Progress — Topology builder with Mermaid/DOT/JSON exports complete; SNMP collector placeholder; inventory tables exist but empty
- **P3**: 🔄 In Progress — Drift detection ✅ Complete; Maintenance windows 🔄; Incident reports ⏳; Disaster recovery ⏳
- **P4**: 🔄 In Progress — Anomaly detection (spike/drift detection complete, EventBus integration complete, IncidentEngine integration complete; unit tests + detector registry pending); Root cause analysis (Planned); Capacity planning (linear regression complete, reports complete, scheduler integration complete; CLI/API + unit tests + SNMP data pending)
- **P5**: Planned — Distributed agents, traffic analysis

---

## Objective 1: Continuous Monitoring & SLA/SLO

**Priority:** High
**Phase:** P0 (foundation) → P1 (SLO calculations)
**Dependencies:** P0 storage, scheduler, monitoring persistence

### Required Features
- Per-device/service availability tracking (24h, 7d, 30d rolling windows)
- Latency percentiles (p50, p95, p99) computed from raw observations — not averaged percentiles
- Packet loss & jitter measurement (where measurable)
- Error categorization (timeout, DNS failure, connection refused, permission denied, etc.)
- Configurable SLO targets per device/service
- Error budget calculation & burn rate detection (fast/slow burn)
- Planned downtime exclusion policy (explicit, auditable)
- Distinction: service failure vs. agent failure vs. no observations
- Retention policies for raw observations vs. aggregates

### Acceptance Criteria
- [x] Scheduler persists every check result to `observations` table
- [x] Availability % matches formula: `successful_checks / total_checks` per window
- [ ] p50/p95/p99 computed via `percentile_cont` or equivalent on raw latency_ms
- [ ] Packet loss & jitter measurement
- [ ] Error budget % = `1 - (actual_availability / target_availability)`
- [ ] Burn rate alert fires when budget consumed faster than threshold
- [ ] Planned maintenance windows exclude checks from SLO math (configurable)
- [ ] Unit tests with synthetic data verify all formulas
- [ ] Integration test: 24h simulated run produces correct SLO dashboard values

### Risks
- High-frequency observations → SQLite write contention; consider batching or separate metrics DB
- Percentile computation on large datasets — may need materialized views or streaming approx (tdigest)

---

## Objective 2: Live Topology Map

**Priority:** High
**Phase:** P2
**Dependencies:** P0 storage, discovery, Objective 5 (SNMP neighbor data)

### Required Features
- Graph: devices, interfaces, IPs, MACs, subnets, gateways
- Edge types: L2 adjacency, L3 routing, gateway dependency, containment (device→interface)
- Each edge: provenance (source: ARP, NDP, SNMP, LLDP, routing table, manual), observed_at, confidence (0–1)
- Visual states: online, degraded, offline, unknown (derived from latest check + dependency)
- Incremental discovery (not full rescan)
- Change history: added/removed nodes/edges with timestamps
- Snapshot comparison (diff between two topologies)
- Export: Mermaid, Graphviz DOT, JSON (for UI consumption)
- No inference of physical links without evidence

### Acceptance Criteria
- [x] Topology built from persisted discovery data + monitoring status
- [x] Every edge has `source_of_information`, `observed_at`, `confidence`
- [x] Mermaid export renders valid diagram in GitHub/GitLab
- [x] Diff between snapshots shows added/removed devices and edges
- [x] Visual state derived from: device check + gateway dependency (if gateway down → dependent devices "degraded" not "offline")
- [ ] Integration test: known topology fixture → export matches expected Mermaid

### Risks
- Graph cycles in dependency logic; need clear DAG for "depends on" edges
- Cardinality: full mesh graphs explode; limit edges to observed relationships

---

## Objective 3: Event & Incident Engine

**Priority:** High
**Phase:** P1
**Dependencies:** P0 storage, scheduler, monitoring, Objective 4 (Alertmanager/Telegram)

### Required Features
- **Event schema** (versioned): id, type, version, event_ts, observed_ts, entity, severity, state, source, evidence, dedup_key, correlation_id, trace_id, secure_metadata
- **Incident lifecycle**: `open → acknowledged → resolved → closed` (valid transitions only)
- Alert rules: down/up, latency threshold, loss threshold, sustained degradation, topology change, collector failure
- Correlation: group by gateway/subnet, suppress downstream when root cause identified
- Silences & maintenance windows (Objective 12)
- Cooldowns & deduplication (dedup_key + time window)
- ACK with identity + timestamp
- Auto-resolve when condition clears
- Telegram adapter: open/ack/resolve notifications, rate limiting, retry with backoff, secrets via env only
- Event persistence: full transition history in `events` table

### Acceptance Criteria
- [x] Event struct serialized to JSON matches schema v1
- [x] Incident state machine enforces valid transitions (tested)
- [ ] Deduplication: same dedup_key within cooldown → no new incident
- [ ] Correlation: simulated gateway failure + 5 downstream devices → 1 root incident, 5 suppressed
- [ ] Telegram: message sent on open/ack/resolve; retry on 429/5xx; no blocking on failure
- [ ] No silent event loss: broadcast channel bounded, backpressure handled
- [ ] Unit tests for state machine, dedup, correlation logic

### Risks
- Correlation ≠ causation: always store evidence + confidence, never auto-claim root cause
- Telegram API limits: implement token bucket, queue with persistence

---

## Objective 4: Deployable Observability Stack

**Priority:** High
**Phase:** P1
**Dependencies:** P0 Dockerfile, docker-compose

### Required Features
- **Dockerfile**: multi-stage Rust build, non-root user, distroless/scratch final, healthcheck, no build tools in final
- **docker-compose.yaml** with profiles:
  - `core`: netwatch + SQLite volume
  - `observability`: Prometheus + Grafana + Alertmanager
  - `messaging`: Kafka + Zookeeper/KRaft
  - `full`: all above
- **Prometheus**: scrape config, retention, rules for netwatch alerts, persistent volume
- **Grafana**: provisioned datasources, dashboards (JSON), persistent volume for dashboards/plugins
- **Alertmanager**: Telegram receiver, inhibition rules, routing tree
- **Netwatch metrics exposed**: availability, latency histograms, scheduler health, collector status, event engine health, API health
- **Cardinality control**: no unbounded labels (IP, hostname, URL, error message, incident ID)
- **Backup/restore**: documented procedure for Prometheus, Grafana, SQLite, Kafka

### Acceptance Criteria
- [x] `docker compose --profile core up` → netwatch API reachable, SQLite persists
- [ ] `docker compose --profile observability up` → Prometheus scrapes netwatch, Grafana loads dashboards
- [ ] `docker compose --profile full up` → all services healthy, Kafka optional for core
- [ ] Stop/start containers → Prometheus/Grafana/SQLite data intact
- [ ] Dashboards provisioned (no manual import): overview, device detail, SLO, topology, incidents
- [ ] Alertmanager routes netwatch alerts to Telegram (test with mock)
- [ ] `docker compose down -v` → volumes removed; `up` → clean slate
- [ ] Resource limits defined (CPU/memory) for each service

### Risks
- Kafka adds significant resource overhead; keep optional
- Grafana provisioning requires writable volume for dashboard updates

---

## Objective 5: Inventory & SNMP Monitoring

**Priority:** High
**Phase:** P2
**Dependencies:** P0 storage, discovery, scheduler

### Required Features
- **Inventory**: routers, switches, printers, APs, servers, generic; site/network/VLAN/tags
- Persistent device record with observed interfaces, vendor/model (from sysObjectID), history
- **SNMPv2c**: community-based (legacy support)
- **SNMPv3**: authPriv (SHA/AES), per-device credentials
- Polling: ifTable/ifXTable (status, speed, in/out octets, errors, discards), CPU, memory, uptime
- Rate calculation: bits/sec from counter deltas (handle 32/64-bit wrap)
- Credential protection: never in logs, API responses, or SQLite plaintext (use env/secrets)
- Concurrency limits & rate limiting per target
- No unauthorized network scanning (target list only from inventory)

### Acceptance Criteria
- [ ] SNMPv3 walk against test agent (simulated) returns expected OIDs
- [ ] Interface metrics stored in `observations` with metric=`if_in_octets`, `if_out_octets`, etc.
- [ ] Utilization % = `(rate * 8 * 100) / ifSpeed`
- [ ] Credentials loaded from env/file at runtime, not in config YAML
- [ ] Rate limit: max N concurrent SNMP sessions, max M req/s per target
- [ ] Integration test with `snmpsim` or mock agent
- [ ] No community strings in logs or API output

### Risks
- SNMPv3 engine ID discovery adds complexity; start with pre-configured
- Vendor MIB variations; use standard IF-MIB + HOST-RESOURCES-MIB first

---

## Objective 6: Traffic Analysis & Consumption

**Priority:** Medium-High
**Phase:** P5
**Dependencies:** Objective 5 (SNMP interface counters), optional NetFlow/sFlow collector

### Required Features
- Top talkers by interface/device (bytes, packets, flows)
- Protocol/service breakdown (from NetFlow/sFlow/IPFIX if available)
- Optional PCAP capture: on-demand, duration/size limited, explicit permission
- Anomaly: new destinations, port scans, connection rate spikes
- Baselines: historical latency, volume, frequency per host/service
- Deviation detection: configurable thresholds, evidence-based
- Correlation with incidents & changes
- No payload capture/storage by default; privacy documented
- Deterministic detectors first; ML optional, bounded cost/latency

### Acceptance Criteria
- [ ] SNMP-based interface ranking works without external collectors
- [ ] NetFlow ingestion (if enabled) populates protocol/service stats
- [ ] PCAP capture requires explicit CLI/API call with TTL/size limits
- [ ] Scan detector: evidence = flow records, threshold = new destinations/min
- [ ] All detectors have: rule ID, data source, threshold, false-positive tunable
- [ ] Detector can be disabled without code change

### Risks
- NetFlow/sFlow requires separate collector (nfdump, goflow2); integrate via API not reimplement
- PCAP needs CAP_NET_ADMIN; document privilege requirements

---

## Objective 7: Intelligent Anomaly Detection

**Priority:** Medium-High
**Phase:** P4
**Dependencies:** Objective 1 (historical baselines), Objective 3 (events)

### Required Features
- Baselines: rolling p50/p95/p99 latency, volume, check frequency per entity
- Deviation types: sudden spike, gradual drift, periodic pattern break
- Evidence: baseline window, current value, z-score or percentile rank
- Configurable: sensitivity, minimum samples, lookback window
- Correlation: anomaly + recent change (deploy, config, topology) + incident
- Each anomaly: rule ID, data points, explanation, confidence
- Detector registry: enable/disable per rule, per entity group
- False positive tracking: operator feedback loop

### Acceptance Criteria
- [ ] Baseline computed from last N observations (configurable)
- [ ] Anomaly event includes: rule, entity, baseline stats, current value, evidence
- [ ] Unit test: synthetic baseline + injected spike → anomaly fired with correct evidence
- [ ] Correlation test: anomaly during known change window → linked in incident timeline
- [ ] No ML dependency in core; optional model behind feature flag

### Risks
- Seasonal patterns (daily/weekly) need sufficient history; start with simple rolling window
- Alert fatigue: tune cooldowns, require sustained deviation

---

## Objective 8: Professional Software Engineering

**Priority:** Critical (transversal)
**Phase:** All
**Dependencies:** None (applies to all code)

### Required Features
- **CI (PR)**: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo build --release`, migration tests, API contract tests, metrics cardinality tests, config validation, Dockerfile/Compose validation, dependency vuln scan, secret detection
- **CD**: GHCR multi-arch images, semantic version tags, SBOM (syft), cosign provenance/signing, release notes, artifacts with checksums
- **Testing**: unit (domain logic), integration (API+SQLite), resilience (DB down, Kafka down, Telegram down, restart mid-check), load (API + scheduler throughput)
- **Security**: API auth (when exposed), SSRF protection, input validation, rate limits, secret redaction, non-root containers, dependency updates, SECURITY.md
- **Migrations**: forward-compatible, tested from empty + from previous version
- **Backups**: documented, tested restore procedure
- **Documentation**: ARCHITECTURE.md, DEVELOPMENT.md, CONFIGURATION.md, API.md, OPERATIONS.md, CONTRIBUTING.md

### Acceptance Criteria
- [x] GitHub Actions workflow blocks PR on any failed check
- [ ] Release workflow publishes to GHCR with `vX.Y.Z`, `sha-<short>`, `latest` (semver only for releases)
- [ ] SBOM attached to release
- [ ] Load test: 1000 concurrent checks, <100ms p99 API latency, <500MB RSS
- [ ] Resilience test: kill DB → scheduler pauses, resumes on recovery; no data loss
- [ ] `cargo audit` / `cargo deny` clean on main
- [ ] All 16 objectives have at least one automated test verifying acceptance criteria

### Risks
- CI time: parallelize, cache cargo target
- CD permissions: use OIDC, least-privilege GITHUB_TOKEN

---

## Objective 9: Distributed & Multi-site Monitoring

**Priority:** High (long-term)
**Phase:** P5
**Dependencies:** Objective 4 (stack), Objective 8 (auth/comms)

### Required Features
- Agent registration with site/location metadata
- Central collector API (ingest observations, events, topology)
- Perspective-aware checks: same target from multiple sites
- Agent health: heartbeat, last check, version, config hash
- Secure comms: mTLS or token auth, no plaintext
- Graceful degradation: agent buffers locally, flushes on reconnect
- Comparative views: latency diff, availability diff per site

### Acceptance Criteria
- [ ] Agent binary registers with central API on startup
- [ ] Central API distinguishes observations by `agent_id` + `site_id`
- [ ] Dashboard shows same target from 3 sites with latency comparison
- [ ] Agent restart → no observation loss (local SQLite buffer)
- [ ] mTLS or token auth enforced on ingest endpoint

### Risks
- Clock sync: require NTP/chrony on agents; use observed_at from agent
- Schema evolution: versioned event contracts, backward compatibility

---

## Objective 10: Root Cause Analysis

**Priority:** High
**Phase:** P4
**Dependencies:** Objective 2 (topology dependencies), Objective 3 (incidents), Objective 11 (change history)

### Required Features
- Dependency graph: service → host → gateway → uplink
- Symptom grouping: all failing checks sharing a dependency
- Hypothesis generation: "gateway X down" (evidence: gateway check failed, 10 downstream devices failing)
- Confidence scoring: direct observation > correlation > heuristic
- Human-confirmed root cause stored with incident
- Timeline: changes + anomalies + checks + incidents merged

### Acceptance Criteria
- [ ] Given gateway failure + 5 device failures → hypothesis "gateway" with confidence > 0.8
- [ ] Hypothesis includes: evidence checks, topology path, time correlation
- [ ] Operator can mark hypothesis as confirmed; stored in incident
- [ ] No hypothesis presented as fact without evidence
- [ ] Unit test: topology fixture + check results → correct hypothesis ranked first

### Risks
- Correlation ≠ causation: always label as "probable cause", store confidence

---

## Objective 11: Change Audit & Drift Detection

**Priority:** Medium-High
**Phase:** P3
**Dependencies:** Objective 2 (topology history), Objective 5 (inventory)

### Required Features
- History: interface IP/MAC changes, route changes, device appearance/disappearance
- Comparison: inventory expectation vs. discovered reality
- Drift report: added/removed/modified entities with timestamps
- Correlation: drift event ↔ incident (did change cause outage?)
- Export: diff report (JSON, Markdown)

### Acceptance Criteria
- [x] Discovery runs record every interface/route/device with timestamp
- [x] Diff between two discovery snapshots shows all changes
- [x] Drift detection: expected inventory (from config) vs. discovered → report
- [ ] Incident timeline includes "configuration change detected at T-5min"
- [x] Export produces valid JSON + human-readable Markdown

### Risks
- Churn: frequent DHCP changes create noise; filter by confidence/persistence

---

## Objective 12: Maintenance & Change Management

**Priority:** Medium-High
**Phase:** P3
**Dependencies:** Objective 3 (silences), Objective 4 (Alertmanager)

### Required Features
- Maintenance calendars (recurring, one-off)
- Selective silences: by device, service, subnet, tag
- Auto-expiry: silence ends at window close
- Distinction: silenced alert (not fired) vs. check still runs (data collected)
- History: all silences with creator, reason, duration

### Acceptance Criteria
- [ ] Schedule silence via API; verify no alert fires during window
- [ ] Checks still execute and persist observations during silence
- [ ] Silence expiry → alerts resume immediately if condition persists
- [ ] Audit log: who created silence, when, for what, expiry

### Risks
- Forgotten silences: alert on silences older than N days

---

## Objective 13: Synthetic Service Monitoring

**Priority:** High
**Phase:** P1
**Dependencies:** P0 scheduler, monitoring, storage

### Required Features
- HTTP/HTTPS: status code, body match, cert validation, redirect chain
- DNS: resolution, record types (A, AAAA, CNAME, MX), DNSSEC validation
- TCP: connect, banner grab, STARTTLS
- TLS: cert expiry, chain validity, OCSP, cipher suite
- Per-phase latency: DNS, TCP, TLS, TTFB, total
- Configurable endpoints with headers, auth, expected response
- Independent of host availability (service can fail while host up)

### Acceptance Criteria
- [x] HTTP check validates status 200 + body contains "OK"
- [x] DNS check resolves A record, verifies IP in expected range
- [x] TLS check warns 30/14/7 days before expiry
- [ ] Metrics: `netwatch_synthetic_http_duration_seconds{phase="dns|tcp|tls|ttfb|total"}`
- [ ] Service status decoupled from host: device down + service check not run → service unknown, not down

### Risks
- TLS verification requires system trust store; document CA bundle handling
- Redirect loops: max redirects configurable

---

## Objective 14: Capacity Planning

**Priority:** Medium
**Phase:** P4
**Dependencies:** Objective 1 (historicals), Objective 5 (SNMP utilization)

### Required Features
- Traffic/utilization trends (linear, exponential fit)
- Saturation thresholds: interface >80%, CPU >80%, memory >85%
- Time-to-limit estimation with confidence interval
- Bottleneck identification: top constrained resources
- Report: available headroom, projected exhaustion date

### Acceptance Criteria
- [ ] Trend computed from last 30d of utilization samples
- [ ] Projection includes: model type, R², assumption (linear growth)
- [ ] Report generated via CLI/API, includes uncertainty disclaimer
- [ ] Unit test: synthetic linear growth → predicted date within 10%

### Risks
- Non-linear growth (step changes): detect and segment

---

## Objective 15: Reproducible Incident Reports

**Priority:** Medium
**Phase:** P3
**Dependencies:** Objective 3 (incident history), Objective 1 (metrics), Objective 11 (changes)

### Required Features
- Output: JSON + Markdown
- Timeline: events, checks, changes, anomalies, silences
- Measurements: before/during/after incident (availability, latency, loss)
- Affected entities: devices, services, dependencies
- Evidence: raw check results, metric snapshots, topology diff
- Probable cause + confidence
- Suggested diagnostic commands (ping, traceroute, snmpwalk)
- Version + data provenance (schema version, collector versions)
- No secrets in report

### Acceptance Criteria
- [ ] `netwatch incident report <incident-id> --format markdown` → valid report
- [ ] Report includes all timeline events with timestamps
- [ ] Metrics section shows pre-incident baseline vs. incident window
- [ ] Diagnostic commands are copy-paste runnable
- [ ] Redaction test: SNMP credentials, Telegram tokens absent from output

### Risks
- Large incidents → large reports; add pagination/section filtering

---

## Objective 16: Verified Disaster Recovery

**Priority:** High
**Phase:** P3
**Dependencies:** Objective 4 (persistent volumes), Objective 8 (backup docs)

### Required Features
- Backup: SQLite (`.backup`), Prometheus (snapshot), Grafana (dashboard JSON + DB), Kafka (if used)
- Restore procedure: documented, scripted
- Integrity: checksum verification, row counts
- RPO/RTO targets defined per component
- Test: CI job or documented drill restores full stack in clean environment
- Retention: configurable per data type
- Encryption at rest for backups (optional, documented)

### Acceptance Criteria
- [ ] `scripts/backup.sh` produces timestamped archives for all persistent data
- [ ] `scripts/restore.sh <backup-dir>` brings up functional stack
- [ ] Post-restore: API serves data, Prometheus has metrics, Grafana has dashboards
- [ ] CI includes restore test (or documented manual drill quarterly)
- [ ] RPO ≤ 1h for SQLite, ≤ 15m for Prometheus (WAL), RTO ≤ 10min

### Risks
- Kafka restore complex; document as optional component
- Prometheus snapshot requires `--storage.tsdb.retention.size` headroom

---

## Cross-Objective Dependencies Graph

```
P0: Config → Storage → Scheduler → Monitoring Persistence
                           ↓
P1: Events → Incidents → Alerts (Telegram/Alertmanager)
      ↓              ↓
P1: Synthetic Checks (Obj 13)
      ↓
P1: Observability Stack (Obj 4)
      ↓
P2: Discovery → Topology (Obj 2) ← SNMP (Obj 5)
      ↓
P3: Change Audit (Obj 11) → Maintenance (Obj 12) → Incident Reports (Obj 15) → DR (Obj 16)
      ↓
P4: Anomalies (Obj 7) → Root Cause (Obj 10) → Capacity (Obj 14)
      ↓
P5: Distributed (Obj 9) → Traffic (Obj 6)
```

---

## Definition of Done (Per Feature)

A feature is **Done** when:
1. Code compiles: `cargo build --release`
2. Lint clean: `cargo clippy --all-targets --all-features -- -D warnings`
3. Format clean: `cargo fmt --check`
4. Unit tests pass (domain logic)
5. Integration tests pass (API + SQLite + real dependencies where feasible)
6. E2E smoke test passes (if applicable)
7. Documentation updated (README, API.md, CONFIGURATION.md, CHANGELOG.md)
8. No secrets in code/config/examples
9. Cardinality policy respected (metrics)
10. Security review: no SSRF, no plaintext secrets, input validation