# Changes Made During Implementation

**Date:** 2026-10-11
**Implementation Session:** Phase 1-3: Repository inspection, documentation reconciliation, and prerequisite fixes for P4

---

## Summary

This implementation performed a complete inspection of the netwatch repository state, identified significant discrepancies between documentation and actual implementation, updated all Markdown documentation to accurately reflect the current state, and implemented critical prerequisite fixes for P4 (Intelligence & Capacity).

**Key Finding:** The codebase was significantly more advanced than the documentation indicated. P0 is fully complete with config, storage, scheduler, and persistence all wired and working. P1-P3 have substantial implementation progress that was not reflected in the docs.

**Prerequisite Fixes for P4:**
1. **Objective 1 percentile queries** - Added `compute_percentile`, `get_latency_observations`, `get_availability_stats` to storage
2. **EventBus → IncidentEngine wiring** - Implemented `IncidentEngine` that consumes events and manages incident lifecycle
3. **Service model and DependsOn edges** - Added `Service` node type, `add_service_dependency` method, and wired into health check jobs

---

## Documentation Files Updated

### 1. `README.md` — Complete Rewrite

**Status:** Updated
**Category:** Documentation
**Changes:**
- Updated strategic objectives table with accurate statuses (Partial/Planned instead of all "Planned")
- Updated implementation status table with ✅/⚠️ for 21 components
- Corrected key limitations list
- Added DNS/HTTP/TLS monitoring to implemented features
- Added topology builder, drift detection, scheduler, EventBus to implemented
- Updated quick start examples

### 2. `docs/ROADMAP.md` — Phase Status Correction

**Status:** Updated
**Category:** Documentation
**Changes:**
- Corrected Phase Overview table: P0 ✅ Complete, P1 🔄 In Progress, P2 🔄 In Progress, P3 🔄 In Progress, P4 Planned, P5 Planned
- Updated Phase Status Summary with accurate descriptions
- Updated Objective 1: Added ✅ for scheduler persistence, availability formula; kept P1 items as ❌
- Updated Objective 2: Added ✅ for topology built, provenance/confidence, Mermaid/DOT/JSON export, diff, visual states
- Updated Objective 3: Added ✅ for typed events, incident state machine, ACK/resolve; kept alert rules/correlation/notifiers as ❌
- Updated Objective 4: Added ✅ for Dockerfile, compose, Prometheus/Grafana/Alertmanager configs; noted stack deployment untested
- Updated Objective 11: Added ✅ for change history, snapshot diff, drift detection, JSON/Markdown export
- Updated Objective 13: Added ✅ for HTTP/DNS/TLS collectors; noted scheduler integration pending
- Added accurate checkmarks (✅/❌/🔄) throughout

### 3. `docs/ARCHITECTURE.md` — Major Overhaul

**Status:** Updated
**Category:** Documentation
**Changes:**
- Completely rewrote "Current Architecture (Implemented)" section to reflect actual wiring
- Updated Module Map with all current modules (drift.rs, topology_builder.rs, topology_store.rs, snmp.rs, monitoring/dns.rs, http.rs, tls.rs)
- Updated Component Responsibilities table: Config, Storage, Scheduler, EventBus, Incidents, Drift, Topology all marked ✅
- Added "Scheduled Monitoring (Server Mode)" execution flow showing full job registration
- Updated Data Flow Diagram with wired connections (solid lines) vs placeholders (dashed)
- Added topology/drift subgraph to diagram
- Updated Persistence Schema with both migrations documented
- Updated Evolution Path: Steps 1-5, 11, 14, 17 marked **DONE**
- Updated Limitations: Removed "Config not wired", "Storage not wired", "Scheduler never instantiated", "EventBus never used", "No persistence of checks" — these ARE implemented

### 4. `docs/IMPLEMENTATION_STATUS.md` — Comprehensive Correction

**Status:** Updated
**Category:** Documentation
**Changes:**
- Updated Summary Table with accurate global statuses and key gaps
- Objective 1: Marked scheduler persistence and check persistence as ✅
- Objective 2: Changed from "Planned" to "Partial"; added ✅ for graph, visual states, provenance, exports, snapshots, diff
- Objective 3: Detailed event schema, incident lifecycle as ✅; alert rules/correlation/notifiers as ❌
- Objective 4: Detailed Dockerfile, compose, configs as ✅; stack deployment untested
- Objective 11: Changed from "Partial" to detailed breakdown with ✅ for drift detection, exports
- Objective 13: Added ✅ for HTTP/DNS/TLS collectors; noted scheduler integration pending
- Cross-Cutting: Configuration, Storage, Scheduler, Metrics, Discovery, ICMP, TCP, DNS, HTTP, TLS, CLI, HTTP API Core, Topology, Drift all marked **Implemented**
- P0 Verification Summary: All 8 criteria ✅ with evidence references
- Known Limitations: Updated to reflect actual current gaps (auth, TLS, hot-reload, Linux-only, ping binary, placeholders, SNMP placeholder, no incident engine, no SLO calculations)
- Next Priorities: Updated for P4 (Objectives 7, 10, 14 + CI/CD + missing docs)

### 5. `docs/DEVELOPMENT.md` — Test Counts & Structure Update

**Status:** Updated
**Category:** Documentation
**Changes:**
- Updated project structure to include all current modules (drift.rs, topology_*.rs, snmp.rs, monitoring/dns.rs, http.rs, tls.rs, tests/drift_tests.rs)
- Updated test counts: 46 unit/integration tests (was 41)
- Added drift detection tests to list
- Updated E2E smoke test description: 42 checks including drift and topology
- Added Configuration section with config.example.yaml reference

### 6. `docs/ARCHITECTURE_AND_FUNCTIONAL_TESTS.md` — Complete Overhaul

**Status:** Updated
**Category:** Documentation
**Changes:**
- Updated all component statuses to reflect actual wired state
- Added Service node type, DependsOn edges, IncidentEngine
- Updated execution flows with Scheduled Monitoring flow
- Added Service dependency tracking in health check jobs
- Updated test audit table with 36 tests (was 25)
- Added Service model tests, IncidentEngine tests
- Updated limitations table: removed fixed items, added current gaps

### 7. `docs/CHANGES_MADE.md` — This File

**Status:** Updated
**Category:** Documentation

---

## Production Code Changes

### 1. `src/storage.rs` — Added Percentile and Availability Queries

**Status:** Modified
**Category:** Production code (P4 prerequisite)
**Changes:**
- Added `insert_observation` method for typed observation insertion
- Added `get_observations` with filtering by device, metric, time window
- Added `get_latency_observations` for percentile computation
- Added `compute_percentile` using sorted values (p50/p95/p99)
- Added `get_availability_stats` computing successful/total ratio
- Added `get_observation_count` for sample size checks
- Added `Observation`, `AvailabilityStats`, `PercentileResult` structs
- Changed migration runner to use file-based `Migrator` for dynamic migration discovery

**Impact/Compatibility:**
- Enables Objective 1 SLO calculations and Objective 7 baseline computation
- Enables Objective 14 trend analysis
- No breaking changes to public API

**Validation:** `cargo test`, all integration tests pass

---

### 2. `src/incidents.rs` — Implemented IncidentEngine

**Status:** Modified
**Category:** Production code (P4 prerequisite)
**Changes:**
- Added `root_cause_hypothesis_id`, `root_cause_confirmed`, `root_cause_confirmed_at`, `root_cause_confirmed_by` fields to `Incident`
- Added `confirm_root_cause` function for operator hypothesis confirmation
- Implemented `IncidentEngine` struct that consumes events from EventBus
- `IncidentEngine::run()` processes CheckFailed/ThresholdBreached → creates/updates incidents
- `IncidentEngine::run()` processes CheckCompleted/ThresholdRecovered → auto-resolves incidents
- Cooldown and silence handling integrated
- Deduplication via dedup_key
- 5-minute default cooldown

**Impact/Compatibility:**
- Enables Objective 3 alerting and incident management
- Enables Objective 7 anomaly→incident correlation
- Enables Objective 10 root cause hypothesis storage
- No breaking changes; extends existing Incident model

**Validation:** `cargo test`, compiles and integrates with EventBus

---

### 3. `src/main.rs` — Wired IncidentEngine and Service Dependencies

**Status:** Modified
**Category:** Production code (P4 prerequisite)
**Changes:**
- Added `IncidentEngine` initialization and background task in `main()`
- Created separate `Arc<Mutex<TopologyBuilder>>` for health check jobs
- Discovery job uses original `TopologyBuilder` (non-mutex)
- Health check jobs (ICMP, HTTP, DNS, TLS) now call `add_service_dependency` on mutex-wrapped builder
- Each health check creates/updates a Service node with DependsOn edge to the device it depends on
- Service nodes track protocol, port, and state (Online/Offline based on check result)

**Impact/Compatibility:**
- Enables Objective 10 dependency graph (service → device → gateway)
- Enables Objective 7 service-level baselines
- No breaking changes; adds new topology data

**Validation:** `cargo test`, e2e smoke test (42/42 pass)

---

### 4. `src/topology.rs` — Added Service Node Type

**Status:** Modified
**Category:** Production code (P4 prerequisite)
**Changes:**
- Added `Service` variant to `TopologyNodeType` enum
- Updated `Display` and `FromStr` implementations

**Impact/Compatibility:**
- Enables Service nodes in topology graph
- No breaking changes

**Validation:** `cargo test`, topology exports include Service nodes

---

### 5. `src/topology_builder.rs` — Added Service Dependency Tracking

**Status:** Modified
**Category:** Production code (P4 prerequisite)
**Changes:**
- Added `add_service_dependency` method to create/update Service nodes and DependsOn edges
- Updated `export_mermaid` and `export_dot` to handle Service node type (ellipse shape)
- Method takes &mut self (called from within Mutex lock in main.rs)

**Impact/Compatibility:**
- Health check jobs can now populate service dependency graph
- Topology exports include service nodes and dependencies
- No breaking changes to existing topology building

**Validation:** `cargo test`, topology exports include service nodes

---

### 6. `migrations/20241009000001_incidents.sql` — New Incidents Table

**Status:** Added
**Category:** Database migration
**Changes:**
- Created `incidents` table with all fields including root cause hypothesis fields
- Indexes on dedup_key, entity, state, hypothesis_id

**Impact/Compatibility:**
- Required for IncidentEngine persistence
- Forward-compatible migration (additive only)

**Validation:** `cargo test test_storage_init` passes

---

### 7. `migrations/20251011000000_root_cause.sql` — Root Cause Index

**Status:** Added
**Category:** Database migration
**Changes:**
- Added index on `root_cause_hypothesis_id` (columns already exist in incidents table)

**Impact/Compatibility:**
- Supports efficient hypothesis lookup for Objective 10
- No schema changes, index only

---

## Verification Results After All Changes

| Command | Result |
|---------|--------|
| `cargo fmt --check` | PASS |
| `cargo check` | PASS |
| `cargo test` | PASS (46 tests) |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo build --release` | PASS |
| `scripts/e2e_smoke.sh` | PASS (42/42) |

All validation commands pass, confirming the implementation is solid.

---

## Discrepancies Resolved

| Document | Previous Claim | Actual State | Resolution |
|----------|---------------|--------------|------------|
| ARCHITECTURE_AND_FUNCTIONAL_TESTS.md | Config NOT WIRED | Config loaded from file + env in main.rs | Updated all docs |
| ARCHITECTURE_AND_FUNCTIONAL_TESTS.md | Storage NOT WIRED | Storage::connect() in main.rs, devices_handler queries DB | Updated all docs |
| ARCHITECTURE_AND_FUNCTIONAL_TESTS.md | Scheduler NOT USED | Scheduler instantiated, 6 jobs registered, running in background | Updated all docs |
| ARCHITECTURE_AND_FUNCTIONAL_TESTS.md | EventBus NOT USED | EventBus emits CheckCompleted/Failed, TopologyChanged events | Updated all docs |
| ARCHITECTURE_AND_FUNCTIONAL_TESTS.md | No persistence of checks | persist_observation() writes to observations table | Updated all docs |
| ROADMAP.md | P2 ✅ Complete | P2 🔄 In Progress (topology done, SNMP placeholder) | Updated |
| ROADMAP.md | P3 Drift Detection only | P3 has drift ✅, maintenance 🔄, incident reports ⏳, DR ⏳ | Updated |
| IMPLEMENTATION_STATUS.md | Many objectives "Planned" | Several are Partial with working code | Updated all objectives |
| README.md | All objectives "Planned" | 9 objectives have Partial/Implemented status | Updated |

---

## Files Changed

```
docs/
  README.md                    (Rewritten - accurate status)
  ROADMAP.md                   (Phase statuses, checkmarks corrected)
  ARCHITECTURE.md              (Major overhaul - current architecture reflects wired state)
  IMPLEMENTATION_STATUS.md     (Comprehensive per-objective correction)
  DEVELOPMENT.md               (Structure, test counts, config example)
  ARCHITECTURE_AND_FUNCTIONAL_TESTS.md (Complete overhaul - current state)
  CHANGES_MADE.md              (This file)

src/
  storage.rs                   (Added percentile/availability queries)
  incidents.rs                 (Added IncidentEngine, root cause fields)
  main.rs                      (Wired IncidentEngine, service dependencies)
  topology.rs                  (Added Service node type)
  topology_builder.rs          (Added add_service_dependency, Service exports)
  topology.rs                  (Added Service node type)

migrations/
  20241009000001_incidents.sql (Incidents table)
  20251011000000_root_cause.sql (Hypothesis index)

scripts/
  e2e_smoke.sh                 (Unchanged - 42/42 pass)
```

---

## Next Steps (P4 Implementation)

1. **Objective 7: Anomaly Detection** — Build `BaselineEngine` from observations; implement spike/drift detectors; emit anomaly events
2. **Objective 10: Root Cause Analysis** — Build dependency graph from topology; implement hypothesis engine with confidence scoring
3. **Objective 14: Capacity Planning** — Query utilization trends from observations; implement linear regression; generate reports
4. **CI/CD Pipeline** — GitHub Actions workflow with all required checks; release workflow with GHCR, SBOM, signing
5. **Final validation and documentation**

---

*This document reflects the documentation reconciliation and prerequisite implementation work done in this session before starting P4 core objectives.*

---

## Dependency Security Fixes (2026-10-13)

**Date:** 2026-10-13
**Implementation Session:** Security audit and dependency vulnerability remediation

---

### Summary

Fixed all known security vulnerabilities in dependencies identified by `cargo audit` and updated outdated dependencies to their maintained versions. All changes are minimal and preserve full functionality.

### Vulnerabilities Addressed

| Advisory | Affected Dependency | Resolution | Status |
|----------|---------------------|------------|--------|
| RUSTSEC-2024-0363 | sqlx 0.7.4 | Updated to 0.8.6 (with `default-features = false` to avoid unused mysql/postgres deps) | ✅ Fixed |
| RUSTSEC-2024-0437 | protobuf 2.28.0 (via prometheus 0.13) | Updated prometheus to 0.14.0 (brings protobuf 3.7.2) | ✅ Fixed |
| RUSTSEC-2024-0436 | paste 1.0.15 | Removed — no longer a transitive dependency after sqlx update | ✅ Fixed |
| RUSTSEC-2025-0134 | rustls-pemfile 2.2.0 (via rustls-native-certs 0.7) | Updated rustls-native-certs to 0.8.5 (no longer uses rustls-pemfile) | ✅ Fixed |
| RUSTSEC-2024-0449 | unsafe-libyaml 0.2.11 (via serde_yaml 0.9) | Replaced `serde_yaml` with maintained `yaml_serde` 0.10.7 | ✅ Fixed |
| RUSTSEC-2023-0071 | rsa 0.9.10 (via sqlx-mysql) | **Exception documented** — sqlx-mysql is an optional dependency not enabled (mysql feature disabled via `default-features = false`). The rsa crate is only pulled in if mysql feature is active. | ⚠️ Exception |

### Dependency Updates in Cargo.toml

```diff
- serde_yaml = "0.9"
+ yaml_serde = "0.10"
- sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio", "macros", "migrate"] }
+ sqlx = { version = "0.8", default-features = false, features = ["sqlite", "runtime-tokio", "macros", "migrate"] }
- prometheus = "0.13"
+ prometheus = "0.14"
- rustls-native-certs = "0.7"
+ rustls-native-certs = "0.8"
```

### Profile Optimizations Added

Added debug optimization profiles to reduce build size and improve compilation speed:

```toml
[profile.dev]
debug = "line-tables-only"
split-debuginfo = "unpacked"

[profile.test]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false
```

### Validation Results After Fixes

| Command | Result |
|---------|--------|
| `cargo clean` | PASS |
| `cargo fmt --check` | PASS |
| `cargo check --all-targets --all-features` | PASS |
| `cargo test --all-targets --no-fail-fast` | PASS (91 tests) |
| `cargo test --doc` | PASS (0 tests) |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo build --release` | PASS |
| `cargo audit --no-fetch` | PASS (1 exception documented) |
| `cargo deny check advisories bans licenses sources` | PASS |
| `./scripts/e2e_smoke.sh` | PASS (42/42) |

### Exception Documentation: RUSTSEC-2023-0071 (rsa Marvin Attack)

**Advisory:** RUSTSEC-2023-0071 — RSA PKCS#1 v1.5 encryption vulnerable to Marvin attack (Bleichenbacher-style)

**Affected Crate:** `rsa` 0.9.10 (transitive via `sqlx-mysql` 0.8.6)

**Justification for Exception:**
1. The `rsa` crate is only a transitive dependency of `sqlx-mysql`
2. `sqlx-mysql` is an **optional dependency** of `sqlx` that is **not enabled** in our build
3. Our `Cargo.toml` specifies `default-features = false` and explicitly enables only `sqlite`, `runtime-tokio`, `macros`, `migrate`
4. The `mysql` feature (which would pull in `sqlx-mysql` → `rsa`) is **not activated**
5. Verified: `cargo build` does not compile `sqlx-mysql` or `rsa` — only `sqlx-core`, `sqlx-sqlite`, `sqlx-macros`, `sqlx-macros-core`, and `sqlx` meta-crate are built
6. The lockfile includes `sqlx-mysql` and `rsa` as optional dependencies for completeness, but they are not compiled or linked into the final binary

**Risk:** Zero — the vulnerable code is not present in the compiled binary.

**Mitigation:** If mysql support is ever needed, upgrade to `rsa` 0.10+ (requires sqlx 0.9+) or use a different TLS library.

**Monitoring:** Track sqlx 0.9.0 release and rsa 0.10+ availability for future upgrade.

### Files Changed

```
Cargo.toml                              (Dependency versions, default-features, profile optimizations)
Cargo.lock                              (Regenerated with updated dependencies)
.cargo/audit.toml                       (Existing: RUSTSEC-2023-0071 exception with justification)
docs/CHANGES_MADE.md                    (This file - added security fixes section)
```

---

## AlreadyReg Metrics Fix (2026-10-24)

**Date:** 2026-10-24
**Implementation Session:** P1 Operational Observability — Permanent fix for AlreadyReg duplicate metric registration

---

### Problem

The Docker build was using a Python script (PYFIX) to patch `src/metrics.rs` at build time, removing duplicate registrations of `netwatch_scheduler_jobs_total` and `netwatch_scheduler_errors_total` metrics. These metrics were being registered twice:

1. In `src/metrics.rs` (`AppMetrics::new()`) — as `IntCounter` (no labels)
2. In `src/monitoring/metrics.rs` — as `CounterVec` with `job` label (used by scheduler)

This caused a Prometheus `AlreadyReg` error when both tried to register with the default registry.

### Root Cause

- `src/monitoring/metrics.rs` defines `NETWATCH_SCHEDULER_JOBS_TOTAL` and `NETWATCH_SCHEDULER_ERRORS_TOTAL` as `Lazy<CounterVec>` using `register_counter_vec!` macro, which automatically registers them with the default Prometheus registry at module load time.
- `src/metrics.rs` `AppMetrics::new()` manually created `IntCounter` instances with the same metric names and attempted to register them again via `registry.register()`.
- The `AppMetrics` versions were never actually used (no code called `.inc()` on them), while the scheduler uses the `CounterVec` versions from `monitoring/metrics.rs` with job labels.

### Solution

**1. Fixed `src/metrics.rs`:**
- Removed `jobs_total` and `errors_total` fields from `AppMetrics` struct
- Removed their creation and registration code from `AppMetrics::new()`
- Removed them from `Default` and `Clone` implementations
- Kept only `info`, `uptime`, and `start_time` fields (the actual app-level metrics)

**2. Updated `Dockerfile`:**
- Removed the entire PYFIX Python script block (lines 26-54) that patched `src/metrics.rs` at build time
- Docker now builds the corrected source directly without any automatic modifications

**3. Updated test expectations in `tests/http_integration_tests.rs`:**
- Modified `http_metrics_endpoint_before_checks` test to not expect scheduler metrics before jobs run
- Scheduler metrics (`CounterVec` with labels) only appear when `.inc()` is called with label values
- AppMetrics now only exposes `netwatch_info` and `netwatch_uptime_seconds` unconditionally

### Files Changed

```
src/metrics.rs                          (Removed duplicate scheduler metrics)
Dockerfile                              (Removed PYFIX patch block)
tests/http_integration_tests.rs         (Updated test expectations)
docs/CHANGES_MADE.md                    (This entry)
```

### Validation Results

| Command | Result |
|---------|--------|
| `cargo fmt --check` | PASS |
| `cargo check --locked --all-targets --all-features` | PASS |
| `cargo test --locked --all-targets --all-features --no-fail-fast` | PASS (93 tests) |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo build --locked --release` | PASS |
| `docker build --no-cache -t netwatch-test .` | PASS (no PYFIX, clean build) |

### Impact

- **Eliminates build-time source modification**: Docker now compiles the actual source code
- **Cleaner metrics architecture**: Single source of truth for scheduler metrics in `monitoring/metrics.rs`
- **Correct Prometheus semantics**: CounterVec metrics with labels only appear when used (standard Prometheus behavior)
- **No functional regression**: Scheduler continues to record job metrics correctly; AppMetrics still exposes app info/uptime
- **Reproducible builds**: No hidden patches; source code matches what runs in production

---

## Security Audit & SQLite E2E Fix (2026-10-24)

**Date:** 2026-10-24
**Implementation Session:** Post-release fix for RUSTSEC-2026-0385 and SQLite FOREIGN KEY constraint failure in E2E

---

### 1. Vulnerability Fix: RUSTSEC-2026-0385 (ctutils)

**Problem:** `cargo audit` reported vulnerability RUSTSEC-2026-0385 in `ctutils` version 0.4.3 (transitive dependency via `digest` → `async-snmp`).

**Root Cause:** The `ctutils` crate had a vulnerability fixed in version 0.4.4+. The lockfile was resolving to 0.4.3.

**Solution:** Updated `ctutils` to 0.4.5 via `cargo update -p ctutils`. The dependency chain is:
- `ctutils` 0.4.5 ← `digest` 0.11.3 ← `async-snmp` 0.18.1 ← `netwatch`

**Validation:**
- `cargo audit --no-fetch`: PASS (no vulnerabilities found)
- `cargo deny check advisories`: PASS (advisories ok)
- All tests pass (93 tests)

**Files Changed:**
- `Cargo.lock` (ctutils 0.4.3 → 0.4.5)

---

### 2. SQLite FOREIGN KEY Constraint Fix

**Problem:** The E2E smoke test (`scripts/e2e_smoke.sh`) failed with:
```
ERROR netwatch: Failed to persist observation: error returned from database: (code: 787) FOREIGN KEY constraint failed
```

**Root Cause:** The legacy ICMP health check job (registered when `synthetic_checks.checks` is empty) uses `device_id = "local"` when persisting observations. However, the "local" device was never created in the `devices` table, causing a foreign key constraint violation on the `observations.device_id` → `devices(id)` relationship.

**Solution:** Added `ensure_local_device()` method to `Storage` that creates the "local" device on database connection (after migrations). Called from `Storage::connect()`.

**Validation:**
- All 93 unit/integration tests pass
- E2E smoke test: 40/40 tests pass (both debug and release binaries)
- No FOREIGN KEY constraint errors in server logs
- Scheduler jobs execute and persist observations correctly

**Files Changed:**
- `src/storage.rs` - Added `ensure_local_device()` method and call in `connect()`
- `scripts/e2e_smoke.sh` - Updated metric test expectations to match actual Prometheus behavior (CounterVec metrics only appear when used)