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
| 7.1 | Baseline computed from last N observations (configurable) | ✅ | `BaselineEngine`, `ObservationRepository`, config `anomaly.lookback_window`, `anomaly.min_samples` | **FAIL** — No baseline engine exists | None | No baseline computation; no rolling window queries on `observations` table | Create `BaselineEngine` with configurable window; add SQL queries for percentile computation | FAIL |
| 7.2 | Anomaly event includes: rule, entity, baseline stats, current value, evidence | ✅ | `AnomalyEvent`, `EventType::AnomalyDetected`, `EventEvidence` with anomaly details | **FAIL** — No anomaly event type | None | `EventType` enum lacks `AnomalyDetected`; no anomaly evidence struct | Add `AnomalyDetected` to `EventType`; create `AnomalyEvidence` with baseline stats | FAIL |
| 7.3 | Unit test: synthetic baseline + injected spike → anomaly fired with correct evidence | ✅ | Test module for anomaly detection | **FAIL** — No implementation to test | None | No detector implementation | Implement detector + tests after 7.1, 7.2 | FAIL |
| 7.4 | Correlation test: anomaly during known change window → linked in incident timeline | ✅ | `AnomalyEvent.correlation_id`, `DriftDetector` events, `Incident` timeline | **BLOCKED** — No incident engine consuming events; no anomaly events | None | Incident engine not connected to EventBus; drift→incident correlation missing | Wire EventBus→IncidentEngine; add anomaly correlation after 7.1-7.3 | BLOCKED |
| 7.5 | No ML dependency in core; optional model behind feature flag | ✅ | `Cargo.toml` features, detector registry | **PASS** — No ML deps currently | N/A | No ML crates in Cargo.toml | Maintain; add feature flag when optional model added | PASS |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 7.6 | Deviation types: sudden spike, gradual drift, periodic pattern break | ✅ | Not implemented | FAIL |
| 7.7 | Evidence: baseline window, current value, z-score or percentile rank | ✅ | Not implemented | FAIL |
| 7.8 | Configurable: sensitivity, minimum samples, lookback window | ✅ | Config lacks anomaly section | FAIL |
| 7.9 | Correlation: anomaly + recent change + incident | ✅ | No change/incident data available | BLOCKED |
| 7.10 | Each anomaly: rule ID, data points, explanation, confidence | ✅ | Not implemented | FAIL |
| 7.11 | Detector registry: enable/disable per rule, per entity group | ✅ | Not implemented | FAIL |
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
| 14.1 | Trend computed from last 30d of utilization samples | ✅ | `CapacityPlanner`, `ObservationRepository`, utilization metrics (`if_in_octets`, `if_out_octets`, CPU, memory) | **FAIL** — No planner; no SNMP utilization data | None | `observations` table exists but only latency metrics stored; no interface utilization data; no SNMP collector | Implement SNMP collector (Obj 5) or use existing latency data for trend demo | FAIL |
| 14.2 | Projection includes: model type, R², assumption (linear growth) | ✅ | `CapacityProjection` struct with model, R², assumption, confidence interval | **FAIL** — No projection code | None | No statistical computation | Implement linear regression with R² | FAIL |
| 14.3 | Report generated via CLI/API, includes uncertainty disclaimer | ✅ | CLI command `netwatch capacity report`, API endpoint `/api/v1/capacity` | **FAIL** — No CLI/API for capacity | None | No capacity module | Create capacity module with CLI/API | FAIL |
| 14.4 | Unit test: synthetic linear growth → predicted date within 10% | ✅ | Test module for capacity planning | **FAIL** — No implementation to test | None | No planner implementation | Implement planner + tests after 14.1-14.2 | FAIL |

### Additional Required Features (from roadmap Required Features)

| # | Feature | Required | Current State | Status |
|---|---------|----------|---------------|--------|
| 14.5 | Traffic/utilization trends (linear, exponential fit) | ✅ | Not implemented | FAIL |
| 14.6 | Saturation thresholds: interface >80%, CPU >80%, memory >85% | ✅ | Config lacks thresholds; no utilization data | FAIL |
| 14.7 | Time-to-limit estimation with confidence interval | ✅ | Not implemented | FAIL |
| 14.8 | Bottleneck identification: top constrained resources | ✅ | Not implemented | FAIL |
| 14.9 | Report: available headroom, projected exhaustion date | ✅ | Not implemented | FAIL |

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
| **7. Anomaly Detection** | **FAIL** | No baseline engine, no anomaly events, no detectors, no config |
| **10. Root Cause Analysis** | **FAIL** | No hypothesis engine, no dependency edges for service→gateway, incident engine not connected |
| **14. Capacity Planning** | **FAIL** | No utilization data (SNMP missing), no planner, no projections |

### Critical Path for P4 Implementation

1. **Objective 1 completion** (SLO/baselines): Add percentile queries on `observations`; this unblocks Obj 7 baselines and Obj 14 trends
2. **Objective 3 completion** (Incident engine): Wire EventBus→IncidentEngine; add alert rules; this unblocks Obj 7 correlation and Obj 10 hypothesis storage
3. **Objective 5** (SNMP): Implement IF-MIB polling for interface utilization; this unblocks Obj 14 utilization trends
4. **Objective 2 enhancement**: Add Service nodes and `DependsOn` edges to topology; this unblocks Obj 10 dependency graph
5. **Objective 11 enhancement**: Add drift→incident correlation; this enriches Obj 7 and Obj 10 timelines

### Recommended P4 Implementation Order

1. **Obj 7.1-7.2**: BaselineEngine + AnomalyEvent (depends on Obj 1 percentile queries — implement those first)
2. **Obj 7.3**: Simple threshold detector (spike detection) + tests
3. **Obj 10.1-10.2**: Dependency graph enhancement + RootCauseAnalyzer + Hypothesis model
4. **Obj 14.1-14.2**: CapacityPlanner with linear regression on available latency data (works without SNMP for demo)
5. **Obj 7.6-7.11**: Additional deviation types, detector registry, config
6. **Obj 10.3**: Hypothesis confirmation API (requires Incident engine wired)
7. **Obj 14.3**: Capacity CLI/API + report

---

## Next Steps

1. Complete Objective 1 percentile queries (immediate prerequisite for Obj 7 & 14)
2. Wire EventBus → IncidentEngine (prerequisite for Obj 7 correlation & Obj 10)
3. Add Service model + `DependsOn` edges to topology (prerequisite for Obj 10)
4. Begin P4 implementation with statistical foundations (baseline engine, linear regression)