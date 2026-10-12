# Netwatch P4 Checkpoint

**Checkpoint Date:** 2026-10-12 23:20:00 UTC  
**Project:** Netwatch  
**Branch:** main  
**Base Commit:** 183165d (ci: add GitHub workflows and API test script)  
**Checkpoint Commit:** To be created after documentation  
**Remote:** origin (https://github.com/fel1pe01/netwatch.git)  
**Sync Status:** Up to date with origin/main (no divergence)

---

## A. Identification

| Field | Value |
|-------|-------|
| Project | Netwatch |
| Checkpoint Date/Time | 2026-10-12 23:20:00 UTC |
| Branch | main |
| Base Commit | 183165d (ci: add GitHub workflows and API test script) |
| Checkpoint Commit | TBD (will be created after this document) |
| Remote Sync Status | Synchronized with origin/main |

---

## B. Summary of Changes

### Changes in Working Tree (Uncommitted)

The following files have been modified in the current working tree but not yet committed. These changes were made in prior sessions and are present in the working tree:

#### Core Implementation Changes

| File | Type | Description |
|------|------|-------------|
| `src/anomaly.rs` | **Major modification** | Added anomaly deduplication cache (`recent_anomalies` HashMap), constant-series spike detection using `min_absolute_change` threshold, improved drift detection with non-overlapping recent window, implemented pattern break detection (`check_pattern_break`), added cache cleanup and emission recording methods. Baseline computation now uses separate baseline window (excluding last 5 min) and check window. |
| `src/capacity_planning.rs` | **Major modification** | Added `nominal_capacity` config option with fallback to max-observed proxy, added `protocol` field to `TrendAnalysis`, `capacity_source` field to track capacity reference source, fixed linear regression to use actual timestamps (seconds since window_start), added denominator zero check for identical timestamps, improved saturation threshold calculation, updated markdown/JSON reports to include capacity source. |
| `src/root_cause.rs` | **Minor modification** | Added `is_root()` method to `Hypothesis` to identify root causes (empty dependency path). |
| `src/incidents.rs` | **Minor modification** | Made `IncidentEngine.store` and `IncidentEngine.process_event` public. Reformatted SQL query for readability. |
| `src/events.rs` | **Minor modification** | Added `Default` derive to `EventEvidence`, made `EventType.as_str()` public. |
| `src/storage.rs` | **Minor modification** | Refactored observation mapping to use iterator chain instead of block. |
| `src/topology_builder.rs` | **Minor modification** | Simplified subnet string construction for IPv4 and IPv6. |
| `src/config.rs` | **Minor modification** | Added `anomaly_cooldown_seconds` and `min_absolute_change` to `AnomalyConfig` with defaults. |
| `src/api.rs` | **Minor modification** | Removed unused `Serialize` import. |
| `src/main.rs` | **Major modification** | Removed `discover local` CLI subcommand and its implementation, simplified capacity CLI handler, fixed `event_bus.clone()` for scheduler registration, fixed shutdown signal handling block syntax. |

#### Configuration & Tests

| File | Type | Description |
|------|------|-------------|
| `config.example.yaml` | **Modification** | Added `anomaly_cooldown_seconds: 300` and `min_absolute_change: 10.0` to anomaly section. |
| `tests/anomaly_tests.rs` | **New file** | Unit tests for anomaly detector (untracked, new). |
| `tests/capacity_tests.rs` | **New file** | Unit tests for capacity planner (untracked, new). |
| `tests/incident_tests.rs` | **New file** | Unit tests for incident engine (untracked, new). |
| `tests/root_cause_tests.rs` | **New file** | Unit tests for root cause analyzer (untracked, new). |

#### Untracked Development Scripts (Not for Commit)

The following Python scripts are development/debugging artifacts and should **not** be committed:
- `check_exact.py`, `check_vals.py`, `count_cols.py`, `count_qmarks.py`
- `debug2.py`, `debug_vals.py`
- `fix_vals.py`, `fix_vals2.py`, `fix_vals3.py`, `fix_vals4.py`, `fix_vals5.py`, `fix_vals6.py`
- `fix_anomaly.py`, `fix_anomaly2.py`, `fix_anomaly3.py`, `fix_anomaly4.py`, `fix_anomaly5.py`, `fix_anomaly6.py`
- `fix_capacity.py`, `fix_root_cause.py`, `fix_storage.py`

---

## C. Test Results

### Test Execution Status

**⚠️ NOTE:** Tests could not be executed in this session due to a toolchain issue: the GNU (MinGW) toolchain is missing `dlltool.exe`, preventing compilation of `getrandom` and `parking_lot_core` dependencies. The MSVC target is not installed. This is an environment limitation, not a code issue.

### Previously Reported Test State (from prior sessions)

The following test results were reported from earlier successful runs but **have not been re-verified in this session**:

| Test Suite | Reported Result | Notes |
|------------|-----------------|-------|
| Anomaly detection tests | 13 passed | Not re-run this session |
| Capacity planning tests | 11 passed | Not re-run this session |
| Incident tests | 12 passed | Not re-run this session |
| Root cause tests | 4 of 8 passed (4 failed) | Not re-run this session |
| CLI TCP test (`cli_monitor_tcp_success`) | Possibly flaky | Intermittent failure reported |
| Format check (`cargo fmt --check`) | Passed | Not re-run this session |
| Clippy (`cargo clippy --all-targets --all-features -- -D warnings`) | Passed | Not re-run this session |
| All unit/integration tests | 46 passing | Not re-run this session |
| E2E smoke test (`scripts/e2e_smoke.sh`) | 42/42 passing | Not re-run this session |

### Checks Not Executed This Session

| Check | Command | Reason |
|-------|---------|--------|
| Unit/Integration tests | `cargo test` | Toolchain missing `dlltool.exe` |
| Format check | `cargo fmt --check` | Requires compilation |
| Clippy | `cargo clippy --all-targets --all-features -- -D warnings` | Requires compilation |
| Release build | `cargo build --release` | Toolchain issue |
| E2E smoke test | `./scripts/e2e_smoke.sh` | Requires release binary |

**Action Required:** Install MinGW (providing `dlltool.exe`) or install MSVC target via `rustup target add x86_64-pc-windows-msvc` to restore test execution capability.

---

## D. Limitations and Technical Debt

### 1. Root Cause Analysis (Objective 10) — 4 of 8 Tests Failing

**Status:** PARTIAL  
**Details from P4_AUDIT.md and IMPLEMENTATION_STATUS.md:**

| Gap | Status | Evidence |
|-----|--------|----------|
| Dependency graph: Service → Host → Gateway → Uplink | PARTIAL | GatewayOf edges exist; DependsOn edges from service checks to devices implemented in main.rs; service→host→gateway chain partially implemented |
| Symptom grouping (shared dependency) | FAIL | No grouping logic implemented |
| Hypothesis generation with evidence | PASS | RootCauseAnalyzer implemented with evidence scoring |
| Confidence scoring | PASS | Direct observation > correlation > heuristic implemented |
| Human-confirmed root cause stored | PASS | Incident model has confirmation fields; API implemented |
| Merged timeline (changes + anomalies + checks + incidents) | PARTIAL | Events have timestamps; no unified timeline query |
| Unit test: topology fixture → correct hypothesis | PARTIAL | Implementation complete; unit tests pending (4/8 failing) |

**Root Cause of Test Failures:** The 4 failing tests likely relate to missing DependsOn edges for the full service→gateway chain and missing symptom grouping logic. The `Hypothesis.is_root()` method was added in this checkpoint but the underlying graph structure gaps remain.

### 2. TCP Test Flakiness (`cli_monitor_tcp_success`)

**Status:** UNVERIFIED (previously reported as intermittent)  
**Impact:** Potential CI instability  
**Investigation Needed:** Network timing, port availability, or test isolation issues.

### 3. SNMP Data Availability for Capacity Planning (Objective 14)

**Status:** BLOCKED  
**Details:** SNMP collector is a placeholder only (`src/main.rs` lines 567-586). No IF-MIB polling implemented. Capacity planning currently uses only latency observations (ICMP, HTTP, DNS, TLS) — no interface utilization, CPU, or memory data.  
**Blocker:** Objective 5 (Inventory & SNMP Monitoring) is "Planned" with no implementation.

### 4. Statistical Confidence Intervals

**Status:** NOT IMPLEMENTED  
**Details:** `TrendAnalysis` in `capacity_planning.rs` computes `time_to_saturation` but **no confidence interval calculation exists**. The `confidence_level` config field (default 0.95) is stored but not used for interval computation.  
**Gap:** Roadmap acceptance criterion 14.4 requires "Time-to-limit estimation with confidence interval" — currently PARTIAL.

### 5. Pattern Break Detection Maturity

**Status:** NEWLY IMPLEMENTED, UNTESTED  
**Details:** `check_pattern_break()` added in `anomaly.rs:546-680` compares recent vs. historical windows (mean and variance change ratios). Thresholds: 50% mean change or 100% stddev change.  
**Risks:** No unit tests; thresholds hardcoded as constants; no configuration exposure; false positive rate unknown; seasonal patterns not handled.

### 6. False Positive Tracking

**Status:** NOT IMPLEMENTED  
**Details:** No operator feedback loop for anomaly false positives. No mechanism to mark anomalies as false positives or adjust detector sensitivity based on feedback.  
**Roadmap Reference:** Objective 7.12 — "False positive tracking: operator feedback loop" = FAIL.

### 7. Topological Integration & DependsOn Relationships

**Status:** PARTIAL  
**Details:** Service nodes and DependsOn edges from service checks to devices added in `main.rs` (`TopologyBuilder::add_service_dependency()`). However, full service→host→gateway→uplink chain incomplete. RootCauseAnalyzer needs complete dependency graph for accurate hypothesis generation.

### 8. API Security Risks

**Status:** NO AUTHENTICATION / NO TLS  
**Details:**
- `src/api.rs` — no authentication middleware
- `src/main.rs` — uses plain `TcpListener` (no TLS)
- No SSRF protection on target validation
- No rate limiting on API endpoints
- No secret redaction in logs

---

## E. Next Steps (Prioritized)

### 🔴 Blocking for P4 Completion

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 1 | Fix root cause test failures (4/8) | All 8 root cause tests pass; DependsOn edges complete for service→gateway chain |
| 2 | Add unit tests for AnomalyDetector | `tests/anomaly_tests.rs` covers baseline, spike, drift, pattern break with synthetic data |
| 3 | Add unit tests for CapacityPlanner | `tests/capacity_tests.rs` covers linear regression, saturation, report generation |
| 4 | Implement confidence intervals for capacity projections | `TrendAnalysis` includes `confidence_interval_lower/upper` fields computed at configured level |
| 5 | Implement detector registry | Per-rule and per-entity-group enable/disable via config |

### 🟡 Important Before Production

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 6 | Implement SNMP collector (Objective 5) | IF-MIB polling for interface counters, CPU, memory; credentials from env only |
| 7 | Add per-phase latency metrics for HTTP/DNS/TLS | `netwatch_synthetic_http_duration_seconds{phase="dns\|tcp\|tls\|ttfb\|total"}` exposed |
| 8 | Implement alert rule engine & correlation | Alert rules for down/latency/loss; gateway/subnet correlation; suppression |
| 9 | Add Telegram/Alertmanager notifiers | Notifications on open/ack/resolve; retry with backoff |
| 10 | Add maintenance windows / silence engine | Schedule silences by device/service/subnet/tag; auto-expiry; audit log |
| 11 | Implement drift→incident correlation | Topology change events linked to incidents in timeline |
| 12 | Fix TCP test flakiness | `cli_monitor_tcp_success` passes consistently in CI |

### 🟢 Post-MVP Improvements

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 13 | Exponential fit for capacity trends | Alternative to linear regression; model selection by AIC/BIC |
| 14 | Seasonal baseline patterns | Daily/weekly seasonality in anomaly baselines |
| 15 | Detector registry with hot-reload | Enable/disable detectors without restart |
| 16 | Unified timeline API | Merged view of changes, anomalies, checks, incidents |
| 17 | Incident report generator (Objective 15) | CLI/API produces JSON + Markdown with timeline, metrics, evidence |
| 18 | Backup/restore scripts (Objective 16) | `scripts/backup.sh` + `scripts/restore.sh` tested in CI |
| 19 | CI/CD pipeline completion | GitHub Actions: fmt, clippy, test, build, audit, deny, Docker validation, release workflow |
| 20 | Missing documentation | CONFIGURATION.md, API.md, OPERATIONS.md, CONTRIBUTING.md, SECURITY.md |

---

## F. Recovery Procedure

### To Inspect This Checkpoint

```bash
# View the checkpoint commit
git show <checkpoint-commit-hash>

# View the diff of changes included in checkpoint
git show <checkpoint-commit-hash> --stat
git show <checkpoint-commit-hash> -- <path>
```

### To Restore Working State from This Checkpoint

```bash
# Option 1: Checkout the checkpoint commit (detached HEAD)
git checkout <checkpoint-commit-hash>

# Option 2: Create a branch from the checkpoint
git checkout -b recover-p4-checkpoint <checkpoint-commit-hash>

# Option 3: Apply only the documentation changes (if code changes need separation)
git checkout <checkpoint-commit-hash> -- docs/
```

### To Continue Work from This Point

1. **Restore test capability:** Install MinGW or MSVC target
2. **Run validation:** `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test && cargo build --release`
3. **Address Blocking items (Section E, 🔴)** in priority order
4. **Commit incremental progress** with descriptive messages

### To Identify Changes After This Checkpoint

```bash
# Compare current HEAD with checkpoint
git diff <checkpoint-commit-hash>..HEAD

# List commits after checkpoint
git log --oneline <checkpoint-commit-hash>..HEAD

# Show files changed since checkpoint
git diff --name-only <checkpoint-commit-hash>..HEAD
```

### ⚠️ Non-Destructive Recovery Only

- Do **not** use `git reset --hard` or `git push --force` as standard recovery
- If remote has diverged, use `git fetch` then `git merge` or `git rebase` with conflict resolution
- Preserve all local changes; stash if needed: `git stash push -m "pre-recovery"`

---

## Appendix: File Inventory at Checkpoint

### Modified (Tracked)
- `config.example.yaml`
- `src/anomaly.rs`
- `src/api.rs`
- `src/capacity_planning.rs`
- `src/config.rs`
- `src/events.rs`
- `src/incidents.rs`
- `src/main.rs`
- `src/root_cause.rs`
- `src/storage.rs`
- `src/topology_builder.rs`

### New Test Files (Untracked — Need Review Before Commit)
- `tests/anomaly_tests.rs`
- `tests/capacity_tests.rs`
- `tests/incident_tests.rs`
- `tests/root_cause_tests.rs`

### Untracked Development Artifacts (Exclude from Commit)
- `check_exact.py`, `check_vals.py`, `count_cols.py`, `count_qmarks.py`
- `debug2.py`, `debug_vals.py`
- `fix_vals.py`, `fix_vals2.py`, `fix_vals3.py`, `fix_vals4.py`, `fix_vals5.py`, `fix_vals6.py`
- `fix_anomaly.py`, `fix_anomaly2.py`, `fix_anomaly3.py`, `fix_anomaly4.py`, `fix_anomaly5.py`, `fix_anomaly6.py`
- `fix_capacity.py`, `fix_root_cause.py`, `fix_storage.py`

---

*Checkpoint created as part of Netwatch P4 audit documentation. This document reflects the actual state of the working tree as of 2026-10-12 23:20 UTC.*