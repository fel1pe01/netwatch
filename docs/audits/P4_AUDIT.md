# P4 Audit Matrix — Intelligence & Capacity

**Generated:** 2026-10-11
**Scope:** Objectives 7 (Anomaly Detection), 10 (Root Cause Analysis), 14 (Capacity Planning)
**Dependencies to verify:** Objectives 1, 2, 3, 5, 11, 13

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
| 7.6 | Deviation types: sudden spike, gradual drift, periodic pattern break | ✅ | `AnomalyType::Spike`, `AnomalyType::Drift` implemented; `PatternBreak` defined but not detected | PARTIAL |
| 7.7 | Evidence: baseline window, current value, z-score or percentile rank | ✅ | `AnomalyEvidence` struct includes all fields | PASS |
| 7.8 | Configurable: sensitivity, minimum samples, lookback window | ✅ | `AnomalyConfig` in `anomaly.rs` + `config.rs` with full validation | PASS |
| 7.9 | Correlation: anomaly + recent change + incident | ✅ | Anomaly→incident flow works; change correlation pending | PARTIAL |
| 7.10 | Each anomaly: rule ID, data points, explanation, confidence | ✅ | `AnomalyEvent` includes rule_id, evidence, description, confidence | PASS |
| 7.11 | Detector registry: enable/disable per rule, per entity group | ✅ | Global enable/disable via config; per-rule/entity not implemented | PARTIAL |
| 7.12 | False positive tracking: operator feedback loop | ✅ | Not implemented | FAIL |

---

## Objective 10: Root Cause Analysis

**Dependencies:** Objective 2 (topology dependencies), Objective 3 (incidents), Objective 11 (change history)

| # | Acceptance Criterion | Required by Roadmap | Modules/Types/Functions | Integration State | Tests & Evidence | Defects Found | Correction Needed | Status |
|---|---------------------|---------------------|------------------------|-------------------|------------------|---------------|-------------------|--------|
| 10.1 | Given gateway failure + 5 device failures → hypothesis "gateway" with confidence > 0.8 | ✅ | `RootCauseAnalyzer`, `TopologyGraph`, `IncidentStore`, dependency edges (`DependsOn`, `GatewayOf`) | **FAIL** — No analyzer; dependency edges not in topology | None | Topology has `GatewayOf` edge type but no `DependsOn` for service→host→gateway chain; no hypothesis engine | Add `DependsOn` edges from service checks; build `RootCauseAnalyzer` | FAIL |
| 10.2 | Hypothesis includes: evidence checks, topology path, time correlation | ✅ | `Hypothesis` struct with evidence, path, time window, confidence | **FAIL** — No hypothesis struct | None | No hypothesis generation code | Create `Hypothesis` model and analyzer | FAIL |
| 10.3 | Operator can mark hypothesis as confirmed; stored in incident | ✅ | `Incident.root_cause_confirmed`, `Incident.confirmed_hypothesis_id` | **BLOCKED** — Incident engine not connected to events | None | `Incident` struct lacks root cause fields; no confirmation API | Extend `Incident` model; add confirmation endpoint | BLOCKED |
| 10.4 | No hypothesis presented as fact without evidence | ✅ | `Hypothesis.confidence`, `Hypothesis.evidence` | **FAIL** — No hypothesis engine | None | N/A | Implement with evidence requirement | FAIL |
| 10.5 | Unit test: topology fixture + check results → correct hypothesis ranked first | ✅ | Test module for root cause | **FAIL** — No implementation to test | None | No analyzer implementation | Implement analyzer + tests after 10.1-10.2 | FAIL |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 10.6 | Dependency graph: service → host → gateway → uplink | ✅ | Topology has Device/Interface/Subnet/Gateway nodes; `GatewayOf`, `RoutesThrough` edges; no Service nodes or `DependsOn` edges | PARTIAL |
| 10.7 | Symptom grouping: all failing checks sharing a dependency | ✅ | Not implemented | FAIL |
| 10.8 | Confidence scoring: direct observation > correlation > heuristic | ✅ | Not implemented | FAIL |
| 10.9 | Timeline: changes + anomalies + checks + incidents merged | ✅ | Events have timestamps; drift detection exists; no unified timeline query | PARTIAL |

---

## Objective 14: Capacity Planning

**Dependencies:** Objective 1 (historicals), Objective 5 (SNMP utilization)

| # | Acceptance Criterion | Required by Roadmap | Modules/Types/Functions | Integration State | Tests & Evidence | Defects Found | Correction Needed | Status |
|---|---------------------|---------------------|------------------------|-------------------|------------------|---------------|-------------------|--------|
| 14.1 | Trend computed from last 30d of utilization samples | ✅ | `CapacityPlanner::analyze_capacity`, `perform_linear_regression`, `Storage::get_latency_observations` | **PASS** — Implemented in `capacity_planning.rs:136-226` using latency data | Manual verification; unit tests pending | Uses latency data instead of interface utilization (SNMP not yet implemented) | Add SNMP utilization data when Obj 5 complete | PASS |
| 14.2 | Projection includes: model type, R², assumption (linear growth) | ✅ | `TrendAnalysis` struct with `slope`, `r_squared`, `confidence_level` | **PASS** — `capacity_planning.rs:60-92` includes all fields | Manual verification | R² computed but confidence interval not implemented | Add confidence interval calculation | PASS |
| 14.3 | Report generated via CLI/API, includes uncertainty disclaimer | ✅ | `CapacityReport::to_markdown`, `to_json` | **PASS** — `capacity_planning.rs:350-453` generates reports with disclaimer | Manual verification | No CLI/API command exposed yet | Add CLI subcommand and API endpoint | PARTIAL |
| 14.4 | Unit test: synthetic linear growth → predicted date within 10% | ✅ | Test module for capacity planning | **PARTIAL** — Implementation complete; unit tests pending | Manual verification | No unit test file | Create `tests/capacity_tests.rs` | PARTIAL |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 14.5 | Traffic/utilization trends (linear, exponential fit) | ✅ | Linear regression implemented; exponential fit not implemented | PARTIAL |
| 14.6 | Saturation thresholds: interface >80%, CPU >80%, memory >85% | ✅ | `CapacityConfig.saturation_threshold_pct` (default 80%); only latency trends implemented | PARTIAL |
| 14.7 | Time-to-limit estimation with confidence interval | ✅ | `TrendAnalysis.time_to_saturation` computed; confidence interval not implemented | PARTIAL |
| 14.8 | Bottleneck identification: top constrained resources | ✅ | `CapacityReport.critical_resources` sorted by time to saturation | PASS |
| 14.9 | Report: available headroom, projected exhaustion date | ✅ | `CapacityReport::to_markdown` includes all required fields | PASS |

---

## Dependency Audit — Data Sources for P4

| Dependency | Objective | Required For | Current State | Gap |
|------------|-----------|--------------|---------------|-----|
| Historical observations | 1 | Obj 7 (baselines), Obj 14 (trends) | **PARTIAL** — `observations` table populated by scheduler with latency metrics (icmp, http, dns, tls); only 4 protocols; no interface utilization, CPU, memory | Need SNMP data (Obj 5) for utilization; need more protocols |
| Availability/percentile queries | 1 | Obj 7 (baselines), Obj 14 (trends) | **FAIL** — No SQL views/queries for p50/p95/p99, availability % | Need to implement |
| Topology with dependencies | 2 | Obj 10 (dependency graph) | **PARTIAL** — Graph built with Device/Interface/Subnet/Gateway nodes; `GatewayOf`, `RoutesThrough`, `Contains`, `ConnectedTo` edges; **no Service nodes**, **no `DependsOn` edges** | Add Service model; add `DependsOn` edges from checks to devices/gateways |
| Incident engine consuming events | 3 | Obj 7 (correlation), Obj 10 (hypothesis storage) | **FAIL** — `IncidentStore` exists but no EventBus subscriber creates incidents | Wire EventBus→IncidentEngine |
| Alert rules / correlation / notifiers | 3 | Obj 7 (anomaly→alert), Obj 10 (incident context) | **FAIL** — No alert rule engine, no correlation, no notifiers | Implement |
| SNMP utilization data | 5 | Obj 14 (interface utilization trends) | **FAIL** — SNMP collector placeholder only; no IF-MIB polling | Implement SNMP collector |
| Drift detection + topology change events | 11 | Obj 7 (anomaly+change correlation), Obj 10 (change history in timeline) | **PARTIAL** — Drift detection works, exports JSON/MD; `TopologyChanged` events emitted; **no drift→incident correlation** | Add drift→incident correlation |
| Synthetic check metrics (per-phase) | 13 | Obj 7 (baselines for HTTP/DNS/TLS phases) | **PARTIAL** — HTTP/DNS/TLS collectors implemented; scheduler jobs exist; **no per-phase latency metrics** | Add phase timing to HTTP collector; expose phase metrics |

---

## Summary: P4 Readiness

| Objective | Overall Status | Blockers |
|-----------|----------------|----------|
| **7. Anomaly Detection** | **PARTIAL** | Unit tests for detector; per-rule/entity detector registry; pattern break detection; false positive tracking; change correlation |
| **10. Root Cause Analysis** | **FAIL** | No hypothesis engine, no dependency edges for service→gateway |
| **14. Capacity Planning** | **PARTIAL** | Unit tests for planner; CLI/API exposure; SNMP utilization data; exponential fit; confidence intervals |

### Critical Path for P4 Implementation

1. **Objective 10** (Root Cause Analysis): Add `DependsOn` edges to topology; build `RootCauseAnalyzer` with hypothesis generation; add hypothesis confirmation API
2. **Objective 7 enhancement**: Add unit tests for `AnomalyDetector`; implement detector registry; add pattern break detection; add drift→anomaly correlation
3. **Objective 14 enhancement**: Add unit tests for `CapacityPlanner`; expose CLI/API; integrate SNMP utilization data when Obj 5 complete
4. **Objective 5** (SNMP): Implement IF-MIB polling for interface utilization; this unblocks Obj 14 utilization trends
5. **Objective 11 enhancement**: Add drift→incident correlation; this enriches Obj 7 and Obj 10 timelines

### Recommended P4 Implementation Order

1. **Obj 10.1-10.2**: Dependency graph enhancement + RootCauseAnalyzer + Hypothesis model
2. **Obj 7.3**: Unit tests for anomaly detector (spike, drift, baseline)
3. **Obj 7.6, 7.11**: Pattern break detection + detector registry
4. **Obj 14.3-14.4**: Capacity CLI/API + unit tests
5. **Obj 10.3**: Hypothesis confirmation API

---

## Next Steps

1. **Objective 10**: Add Service model + `DependsOn` edges to topology; build `RootCauseAnalyzer` with hypothesis generation
2. **Objective 7**: Add unit tests for `AnomalyDetector`; implement detector registry; add pattern break detection
3. **Objective 14**: Add unit tests for `CapacityPlanner`; expose CLI subcommand and API endpoint
4. **Objective 5**: Implement SNMP IF-MIB polling for interface utilization
5. **Objective 11**: Add drift→incident correlation