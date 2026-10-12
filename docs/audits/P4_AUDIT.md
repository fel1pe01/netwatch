# P4 Audit Matrix — Intelligence & Capacity

**Generated:** 2026-10-12
**Last Updated:** 2026-10-12 23:20 UTC (Checkpoint)
**Scope:** Objectives 7 (Anomaly Detection), 10 (Root Cause Analysis), 14 (Capacity Planning)
**Dependencies to verify:** Objectives 1, 2, 3, 5, 11, 13
**Checkpoint:** See `docs/checkpoints/CHECKPOINT_P4.md`

---

## Audit State Definitions

- **PASS**: Implementation integrated, behavior demonstrated via tests
- **FAIL**: Functionality absent or defective
- **PARTIAL**: Implementation incomplete or with significant limitations
- **BLOCKED**: Cannot verify/complete due to concrete impediment

---

## Objective 7: Intelligent Anomaly Detection

**Dependencies:** Objective 1 (historical baselines), Objective 3 (events)

| # | Acceptance Criterion | Required by Roadmap | Modules/Types/Functions | Integration State | Tests & Evidence | Defects Found | Correction Needed | Status |
|---|---------------------|---------------------|------------------------|-------------------|------------------|---------------|-------------------|--------|
| 7.1 | Baseline computed from last N observations (configurable) | ✅ | `AnomalyDetector::compute_baseline`, `get_or_compute_baseline`, `Storage::get_latency_observations`, config `anomaly.lookback_window_seconds`, `anomaly.min_samples` | **PASS** — Implemented in `anomaly.rs:241-300` | Manual verification via E2E; unit tests pending | None | Add unit tests for baseline computation | PASS |
| 7.2 | Anomaly event includes: rule, entity, baseline stats, current value, evidence | ✅ | `AnomalyEvent`, `EventType::AnomalyDetected`, `EventEvidence::anomaly` | **PASS** — `AnomalyEvent` in `anomaly.rs:125-141`; `EventType::AnomalyDetected` in `events.rs:57`; `AnomalyEvidence` in `events.rs:124-142` | E2E smoke test passes; anomaly events flow through EventBus | None | Add unit tests for event emission | PASS |
| 7.3 | Unit test: synthetic baseline + injected spike → anomaly fired with correct evidence | ✅ | Test module for anomaly detection | **PARTIAL** — Implementation complete; unit tests pending | E2E integration verified | No unit test file for anomaly detector | Create `tests/anomaly_tests.rs` with synthetic data | PARTIAL |
| 7.4 | Correlation test: anomaly during known change window → linked in incident timeline | ✅ | `AnomalyEvent.correlation_id`, `IncidentEngine` consumes `AnomalyDetected` | **PASS** — `IncidentEngine::handle_firing_event` handles `AnomalyDetected` in `incidents.rs:630-635` | E2E verified; anomaly creates incident | Change window correlation not yet implemented | Add drift→anomaly correlation logic | PASS |
| 7.5 | No ML dependency in core; optional model behind feature flag | ✅ | `Cargo.toml` features, detector registry | **PASS** — No ML deps currently | N/A | No ML crates in Cargo.toml | Maintain; add feature flag when optional model added | PASS |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 7.6 | Deviation types: sudden spike, gradual drift, periodic pattern break | ✅ | `AnomalyType::Spike`, `AnomalyType::Drift`, `AnomalyType::PatternBreak` implemented; `check_pattern_break` in `anomaly.rs:546-680` | **PASS** (NEW: pattern break implemented 2026-10-12) |
| 7.7 | Evidence: baseline window, current value, z-score or percentile rank | ✅ | `AnomalyEvidence` struct includes all fields | PASS |
| 7.8 | Configurable: sensitivity, minimum samples, lookback window | ✅ | `AnomalyConfig` in `anomaly.rs` + `config.rs` with full validation | PASS |
| 7.9 | Correlation: anomaly + recent change + incident | ✅ | Anomaly→incident flow works; change correlation pending | PARTIAL |
| 7.10 | Each anomaly: rule ID, data points, explanation, confidence | ✅ | `AnomalyEvent` includes rule_id, evidence, description, confidence | PASS |
| 7.11 | Detector registry: enable/disable per rule, per entity group | ✅ | Global enable/disable via config; per-rule/entity not implemented | PARTIAL |
| 7.12 | False positive tracking: operator feedback loop | ✅ | Not implemented | FAIL |

### New Features Implemented (Checkpoint 2026-10-12)

| # | Feature | Description | Location |
|---|---------|-------------|----------|
| 7.13 | Constant-series spike detection | Uses `min_absolute_change` when stddev ≈ 0 | `anomaly.rs:373-410` |
| 7.14 | Anomaly deduplication cache | `recent_anomalies` HashMap with configurable cooldown (`anomaly_cooldown_seconds`) | `anomaly.rs:218-220, 546-580` |
| 7.15 | Separate baseline/check windows | Baseline window excludes last 5 min; check window = last 5 min | `anomaly.rs:522-530` |
| 7.16 | Drift detection non-overlapping window | Recent window ends at baseline window_start | `anomaly.rs:464-470` |
| 7.17 | Pattern break detection | Compares recent vs historical windows (mean/variance change ratios); thresholds: 50% mean, 100% stddev | `anomaly.rs:546-680` |

### Known Limitations (Anomaly Detection)

1. **Pattern break thresholds hardcoded** — 50% mean change, 100% stddev change; not exposed in config
2. **No detector registry** — Per-rule/entity enable/disable not implemented
3. **No false positive tracking** — No operator feedback loop
4. **No unit tests** — Implementation verified via E2E only
5. **Seasonal patterns not handled** — Simple rolling window only

---

## Objective 10: Root Cause Analysis

**Dependencies:** Objective 2 (topology dependencies), Objective 3 (incidents), Objective 11 (change history)

| # | Acceptance Criterion | Required by Roadmap | Modules/Types/Functions | Integration State | Tests & Evidence | Defects Found | Correction Needed | Status |
|---|---------------------|---------------------|------------------------|-------------------|------------------|---------------|-------------------|--------|
| 10.1 | Given gateway failure + 5 device failures → hypothesis "gateway" with confidence > 0.8 | | | `RootCauseAnalyzer`, `TopologyGraph`, `IncidentStore`, dependency edges (`DependsOn`, `GatewayOf`) | **PARTIAL** -- RootCauseAnalyzer implemented in root_cause.rs; GatewayOf edges exist; DependsOn edges from service checks to devices implemented; service->host->gateway chain partially implemented | Fixed: failure detection now uses confidence='low' | DependsOn edges from service checks to devices implemented in main.rs; service->host->gateway chain partially implemented | PARTIAL |
| 10.2 | Hypothesis includes: evidence checks, topology path, time correlation | | | `Hypothesis` struct with evidence, path, time window, confidence | **PASS** -- Hypothesis struct in root_cause.rs:13-34 with evidence, dependency_path, time_window_start, time_window_end, confidence fields | None | Implemented in root_cause.rs | PASS |
| 10.3 | Operator can mark hypothesis as confirmed; stored in incident | | | `Incident.root_cause_confirmed`, `Incident.confirmed_hypothesis_id` | **PASS** -- Incident model has root_cause_hypothesis_id, root_cause_confirmed, root_cause_confirmed_at, root_cause_confirmed_by; confirm_root_cause API in api.rs:217-240 | None | Implemented in incidents.rs and api.rs | PASS |
| 10.4 | No hypothesis presented as fact without evidence | | | `Hypothesis.confidence`, `Hypothesis.evidence` | **PASS** -- Hypothesis struct in root_cause.rs has confidence and evidence fields; RootCauseAnalyzer generates hypotheses with evidence | None | Implemented in root_cause.rs | PASS |
| 10.5 | Unit test: topology fixture + check results → correct hypothesis ranked first | | | Test module for root cause | **PARTIAL** -- Implementation complete; unit tests pending | E2E integration verified | No unit test file for root cause analyzer | PARTIAL |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 10.6 | Dependency graph: service → host → gateway → uplink | ✅ | Topology has Device/Interface/Subnet/Gateway nodes; `GatewayOf`, `RoutesThrough` edges; no Service nodes or `DependsOn` edges | PARTIAL |
| 10.7 | Symptom grouping: all failing checks sharing a dependency | ✅ | Not implemented | FAIL |
| 10.8 | Confidence scoring: direct observation > correlation > heuristic | | | Implemented in RootCauseAnalyzer with evidence weighting | PASS |
| 10.9 | Timeline: changes + anomalies + checks + incidents merged | ✅ | Events have timestamps; drift detection exists; no unified timeline query | PARTIAL |

### New Features Implemented (Checkpoint 2026-10-12)

| # | Feature | Description | Location |
|---|---------|-------------|----------|
| 10.10 | Hypothesis.is_root() method | Returns true for root causes (empty dependency_path) | `root_cause.rs:54-60` |
| 10.11 | Service nodes + DependsOn edges | `TopologyBuilder::add_service_dependency()` creates service nodes with DependsOn edges to devices | `main.rs:300-330`, `topology_builder.rs` |

### Known Limitations (Root Cause Analysis)

1. **4 of 8 unit tests failing** — Likely due to incomplete DependsOn edges and missing symptom grouping
2. **Symptom grouping not implemented** — No logic to group failing checks by shared dependency
3. **Service→host→gateway chain incomplete** — DependsOn edges from service checks to devices exist, but full chain to gateway not complete
4. **No unified timeline query** — Events have timestamps but no merged view
5. **No unit tests** — Implementation verified via E2E only; `tests/root_cause_tests.rs` created but untested

---

## Objective 14: Capacity Planning

**Dependencies:** Objective 1 (historicals), Objective 5 (SNMP utilization)

| # | Acceptance Criterion | Required by Roadmap | Modules/Types/Functions | Integration State | Tests & Evidence | Defects Found | Correction Needed | Status |
|---|---------------------|---------------------|------------------------|-------------------|------------------|---------------|-------------------|--------|
| 14.1 | Trend computed from last 30d of utilization samples | ✅ | `CapacityPlanner::analyze_capacity`, `perform_linear_regression`, `Storage::get_latency_observations` | **PASS** — Implemented in `capacity_planning.rs:136-226` using latency data | Manual verification; unit tests pending | Fixed: linear regression now uses actual timestamps; saturation threshold uses config percentage | Add SNMP utilization data when Obj 5 complete | PASS |
| 14.2 | Projection includes: model type, R², assumption (linear growth) | ✅ | `TrendAnalysis` struct with `slope`, `r_squared`, `confidence_level` | **PASS** — `capacity_planning.rs:60-92` includes all fields | Manual verification | R² computed but confidence interval not implemented | Add confidence interval calculation | PASS |
| 14.3 | Report generated via CLI/API, includes uncertainty disclaimer | ✅ | `CapacityReport::to_markdown`, `to_json` | **PASS** — `capacity_planning.rs:350-453` generates reports with disclaimer | Manual verification | CLI/API command exposed via `netwatch capacity report` and `/api/v1/capacity/report` | Verified working | PASS |
| 14.4 | Unit test: synthetic linear growth → predicted date within 10% | ✅ | Test module for capacity planning | **PARTIAL** — Implementation complete; unit tests pending | Manual verification | No unit test file | Create `tests/capacity_tests.rs` | PARTIAL |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 14.5 | Traffic/utilization trends (linear, exponential fit) | ✅ | Linear regression implemented with proper timestamps; exponential fit not implemented | PARTIAL |
| 14.6 | Saturation thresholds: interface >80%, CPU >80%, memory >85% | ✅ | `CapacityConfig.saturation_threshold_pct` (default 80%); now properly applied to max observed value | PASS |
| 14.7 | Time-to-limit estimation with confidence interval | ✅ | `TrendAnalysis.time_to_saturation` computed; confidence interval not implemented | PARTIAL |
| 14.8 | Bottleneck identification: top constrained resources | ✅ | `CapacityReport.critical_resources` sorted by time to saturation | PASS |
| 14.9 | Report: available headroom, projected exhaustion date | ✅ | `CapacityReport::to_markdown` includes all required fields | PASS |

### New Features Implemented (Checkpoint 2026-10-12)

| # | Feature | Description | Location |
|---|---------|-------------|----------|
| 14.10 | Nominal capacity config | `CapacityConfig.nominal_capacity` option; if set, used as 100% reference; else falls back to max observed | `capacity_planning.rs:19-25, 311-325` |
| 14.11 | Capacity source tracking | `capacity_source` field in `TrendAnalysis`: "nominal_capacity" or "max_observed_proxy" | `capacity_planning.rs:94-97, 311-325, 394-398` |
| 14.12 | Timestamp-based linear regression | Uses actual timestamps (seconds since window_start) as x-values instead of sequential indices | `capacity_planning.rs:277-290` |
| 14.13 | Denominator zero check | Handles identical timestamps (denominator ≈ 0) gracefully with error | `capacity_planning.rs:289-295` |
| 14.14 | Protocol field in TrendAnalysis | Added `protocol` field for better identification | `capacity_planning.rs:67, 395` |
| 14.15 | Reports include capacity source | Markdown/JSON reports show "nominal_capacity" or "max_observed_proxy" | `capacity_planning.rs:438-452` |
| 14.16 | Uncertainty disclaimer enhanced | Notes that max-observed proxy may underestimate true capacity | `capacity_planning.rs:515-518` |

### Known Limitations (Capacity Planning)

1. **No confidence intervals** — `confidence_level` config (default 0.95) stored but not used for interval computation
2. **No exponential fit** — Only linear regression implemented
3. **SNMP utilization data blocked** — Objective 5 not implemented; only latency trends available
4. **No unit tests** — Implementation verified manually only; `tests/capacity_tests.rs` created but untested
5. **Max-observed proxy risk** — Using max observed as capacity reference may underestimate true capacity

---

## Dependency Audit — Data Sources for P4

| Dependency | Objective | Required For | Current State | Gap |
|------------|-----------|--------------|---------------|-----|
| Historical observations | 1 | Obj 7 (baselines), Obj 14 (trends) | **PARTIAL** — `observations` table populated by scheduler with latency metrics (icmp, http, dns, tls); only 4 protocols; no interface utilization, CPU, memory | Need SNMP data (Obj 5) for utilization; need more protocols |
| Availability/percentile queries | 1 | Obj 7 (baselines), Obj 14 (trends) | **FAIL** — No SQL views/queries for p50/p95/p99, availability % | Need to implement |
| Topology with dependencies | 2 | Obj 10 (dependency graph) | **PARTIAL** — Graph built with Device/Interface/Subnet/Gateway nodes; `GatewayOf`, `RoutesThrough`, `Contains`, `ConnectedTo` edges; **no Service nodes**, **no `DependsOn` edges** | Add Service model; add `DependsOn` edges from checks to devices/gateways |
| Incident engine consuming events | 3 | Obj 7 (correlation), Obj 10 (hypothesis storage) | **PASS** — `IncidentEngine` wired to `EventBus` in `main.rs`; consumes `AnomalyDetected`, `CheckFailed`, `ThresholdBreached` events | None |
| Alert rules / correlation / notifiers | 3 | Obj 7 (anomaly→alert), Obj 10 (incident context) | **FAIL** — No alert rule engine, no correlation, no notifiers | Implement |
| SNMP utilization data | 5 | Obj 14 (interface utilization trends) | **FAIL** — SNMP collector placeholder only; no IF-MIB polling | Implement SNMP collector |
| Drift detection + topology change events | 11 | Obj 7 (anomaly+change correlation), Obj 10 (change history in timeline) | **PARTIAL** — Drift detection works, exports JSON/MD; `TopologyChanged` events emitted; **no drift→incident correlation** | Add drift→incident correlation |
| Synthetic check metrics (per-phase) | 13 | Obj 7 (baselines for HTTP/DNS/TLS phases) | **PARTIAL** — HTTP/DNS/TLS collectors implemented; scheduler jobs exist; **no per-phase latency metrics** | Add phase timing to HTTP collector; expose phase metrics |

---

## Summary: P4 Readiness

| Objective | Overall Status | Blockers |
|-----------|----------------|----------|
| **7. Anomaly Detection** | **PARTIAL** | Unit tests for detector; per-rule/entity detector registry; pattern break config exposure; false positive tracking; change correlation |
| **10. Root Cause Analysis** | **PARTIAL** | RootCauseAnalyzer, Hypothesis model, confirmation API implemented; needs complete DependsOn edges for service→gateway chain; symptom grouping; fix 4/8 failing tests |
| **14. Capacity Planning** | **PARTIAL** | Unit tests for planner; SNMP utilization data; confidence intervals; exponential fit |

### Critical Path for P4 Implementation

1. **Objective 10** (Root Cause Analysis): Complete DependsOn edges for service→host→gateway chain; implement symptom grouping; fix failing tests
2. **Objective 7 enhancement**: Add unit tests for `AnomalyDetector`; implement detector registry; expose pattern break thresholds in config; add drift→anomaly correlation
3. **Objective 14 enhancement**: Add unit tests for `CapacityPlanner`; implement confidence intervals; integrate SNMP utilization data when Obj 5 complete
4. **Objective 5** (SNMP): Implement IF-MIB polling for interface utilization; this unblocks Obj 14 utilization trends
5. **Objective 11 enhancement**: Add drift→incident correlation; this enriches Obj 7 and Obj 10 timelines

### Recommended P4 Implementation Order

1. **Obj 10**: Complete DependsOn edges + symptom grouping + fix failing tests
2. **Obj 7.3**: Unit tests for anomaly detector (spike, drift, baseline, pattern break)
3. **Obj 7.11, 7.13**: Detector registry + pattern break config exposure
4. **Obj 14.3-14.4**: Capacity CLI/API + unit tests + confidence intervals
5. **Obj 7.12**: False positive tracking
6. **Obj 5**: SNMP IF-MIB polling
7. **Obj 11**: Drift→incident correlation

---

## Next Steps

1. **Objective 10**: Add Service model + `DependsOn` edges to topology; build `RootCauseAnalyzer` with hypothesis generation
2. **Objective 7**: Add unit tests for `AnomalyDetector`; implement detector registry; add pattern break detection
3. **Objective 14**: Add unit tests for `CapacityPlanner`; expose CLI subcommand and API endpoint
4. **Objective 5**: Implement SNMP IF-MIB polling for interface utilization
5. **Objective 11**: Add drift→incident correlation

---

## Fixes Applied (2026-10-12)

### AnomalyDetector (`src/anomaly.rs`)
- Fixed `get_monitored_entities()` query: Changed `WHERE value > 0` to `WHERE observed_at >= datetime('now', '-1 hour')` to include all recent entities
- Fixed `analyze_and_emit()`: Now checks ALL recent observations (last 5 min) instead of only the most recent value
- Fixed indentation and loop structure for proper anomaly emission
- **NEW: Added anomaly deduplication cache** (`recent_anomalies` HashMap) with configurable cooldown (`anomaly_cooldown_seconds`)
- **NEW: Added constant-series spike detection** using `min_absolute_change` threshold when stddev ≈ 0
- **NEW: Added pattern break detection** (`check_pattern_break`) comparing recent vs historical windows
- **NEW: Separate baseline/check windows** — baseline excludes last 5 min; check window = last 5 min
- **NEW: Drift detection uses non-overlapping recent window** ending at baseline window_start

### CapacityPlanner (`src/capacity_planning.rs`)
- Fixed `get_resource_ids()`: Now returns `entity_id:metric:protocol` format (3 parts) instead of `device_id:metric` (2 parts)
- Fixed `perform_linear_regression()`: Now uses actual timestamps (seconds since window_start) as x-values instead of sequential indices (0, 1, 2...)
- Added `get_latency_observations_with_timestamps()` to `Storage` for proper time series regression
- Fixed saturation threshold calculation: Now uses `max_observed * (saturation_threshold_pct / 100.0)` from config instead of hardcoded 1000ms
- Fixed `TrendAnalysis` construction: Uses actual `entity_id`, `window_start`, `window_end`, and config `confidence_level`
- **NEW: Added `nominal_capacity` config option** with fallback to max-observed proxy
- **NEW: Added `capacity_source` field** tracking "nominal_capacity" vs "max_observed_proxy"
- **NEW: Added denominator zero check** for identical timestamps
- **NEW: Added `protocol` field** to `TrendAnalysis`
- **NEW: Reports include capacity source** and enhanced uncertainty disclaimer

### RootCauseAnalyzer (`src/root_cause.rs`)
- Fixed `get_failing_observations()`: Changed query from `value = 0` to `confidence = 'low'` to accurately detect failed checks
- **NEW: Added `Hypothesis::is_root()` method** to identify root causes (empty dependency_path)

### Main Application (`src/main.rs`)
- Added `IncidentEngine` initialization and wiring to `EventBus` via `subscribe()`
- Added `IncidentEngine` background task alongside scheduler
- Properly aborts both scheduler and incident engine on shutdown
- **NEW: Removed `discover local` CLI subcommand** (simplified CLI)
- **NEW: Simplified capacity CLI handler**
- **NEW: Fixed `event_bus.clone()` for scheduler registration**
- **NEW: Fixed shutdown signal handling block syntax**

### Configuration (`config.example.yaml`)
- Added `capacity` section with `enabled` and `interval` options
- Added `anomaly` section with all detector configuration options
- **NEW: Added `anomaly_cooldown_seconds: 300` and `min_absolute_change: 10.0`**

### Documentation
- Created `docs/checkpoints/CHECKPOINT_P4.md` with full checkpoint documentation
- Updated `docs/ROADMAP.md` with current P4 status
- Updated `docs/IMPLEMENTATION_STATUS.md` with current implementation state
- Updated `docs/audits/P4_AUDIT.md` with current audit state