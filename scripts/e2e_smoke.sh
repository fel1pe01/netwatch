#!/usr/bin/env bash
set -uo pipefail

# Netwatch E2E Smoke Test
# Tests the actual release binary: CLI commands, HTTP endpoints, and Prometheus metrics

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINARY="${PROJECT_ROOT}/target/release/netwatch"
LOG_FILE="/tmp/netwatch_e2e.log"
PID_FILE="/tmp/netwatch_e2e.pid"
BASE_URL=""
SERVER_PID=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

PASS_COUNT=0
FAIL_COUNT=0

log() {
    echo -e "${GREEN}[INFO]${NC} $*" | tee -a "${LOG_FILE}"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*" | tee -a "${LOG_FILE}"
}

fail() {
    echo -e "${RED}[FAIL]${NC} $*" | tee -a "${LOG_FILE}"
    ((FAIL_COUNT++))
}

pass() {
    echo -e "${GREEN}[PASS]${NC} $*" | tee -a "${LOG_FILE}"
    ((PASS_COUNT++))
}

cleanup() {
    if [[ -n "${SERVER_PID}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
        log "Stopping server (PID: ${SERVER_PID})"
        kill "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
}

trap cleanup EXIT INT TERM

check_binary() {
    if [[ ! -f "${BINARY}" ]]; then
        fail "Binary not found at ${BINARY}. Run 'cargo build --release' first."
        exit 1
    fi
    log "Using binary: ${BINARY}"
}

start_server() {
    log "Starting netwatch server..."
    # Use a temporary database file for each test run
    local test_db="/tmp/netwatch_e2e_${RANDOM}.sqlite"
    NETWATCH_STORAGE_PATH="${test_db}" "${BINARY}" > "${LOG_FILE}" 2>&1 &
    SERVER_PID=$!
    echo "${SERVER_PID}" > "${PID_FILE}"

    # Wait for server to be ready
    local max_attempts=30
    local attempt=0
    while [[ ${attempt} -lt ${max_attempts} ]]; do
        if curl -s -f "http://localhost:8080/health" >/dev/null 2>&1; then
            BASE_URL="http://localhost:8080"
            log "Server started on ${BASE_URL} (PID: ${SERVER_PID})"
            return 0
        fi
        sleep 0.2
        ((attempt++))
    done

    fail "Server failed to start within ${max_attempts} attempts"
    cat "${LOG_FILE}"
    exit 1
}

# Test helper: assert HTTP status
assert_status() {
    local url="$1"
    local expected="$2"
    local desc="$3"
    local actual
    actual=$(curl -s -o /dev/null -w "%{http_code}" "${url}" 2>/dev/null || echo "000")
    if [[ "${actual}" == "${expected}" ]]; then
        pass "${desc}: HTTP ${actual}"
    else
        fail "${desc}: expected HTTP ${expected}, got ${actual}"
    fi
}

# Test helper: assert JSON field value
assert_json_field() {
    local url="$1"
    local field="$2"
    local expected="$3"
    local desc="$4"
    local value
    value=$(curl -s "${url}" 2>/dev/null | jq -r "${field}" 2>/dev/null || echo "null")
    if [[ "${value}" == "${expected}" ]]; then
        pass "${desc}: ${field} = ${value}"
    else
        fail "${desc}: ${field} expected '${expected}', got '${value}'"
    fi
}

# Test helper: assert JSON field exists and is not null
assert_json_field_exists() {
    local url="$1"
    local field="$2"
    local desc="$3"
    local value
    value=$(curl -s "${url}" 2>/dev/null | jq -r "${field}" 2>/dev/null || echo "null")
    if [[ "${value}" != "null" && "${value}" != "" ]]; then
        pass "${desc}: ${field} exists (${value})"
    else
        fail "${desc}: ${field} missing or null"
    fi
}

# Test helper: assert metrics contains string
assert_metrics_contains() {
    local pattern="$1"
    local desc="$2"
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
    if echo "${body}" | grep -q "${pattern}"; then
        pass "${desc}: found '${pattern}'"
    else
        fail "${desc}: pattern '${pattern}' not found in metrics"
    fi
}

# Test helper: assert metrics does NOT contain string
assert_metrics_not_contains() {
    local pattern="$1"
    local desc="$2"
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
    if ! echo "${body}" | grep -q "${pattern}"; then
        pass "${desc}: correctly absent '${pattern}'"
    else
        fail "${desc}: pattern '${pattern}' unexpectedly found in metrics"
    fi
}

# ===== CLI TESTS =====
test_cli_version() {
    local output
    output=$("${BINARY}" --version 2>&1)
    if echo "${output}" | grep -q "netwatch"; then
        pass "CLI --version"
    else
        fail "CLI --version: unexpected output: ${output}"
    fi
}

test_cli_help() {
    local output
    output=$("${BINARY}" --help 2>&1)
    if echo "${output}" | grep -q "Netwatch network monitoring tool"; then
        pass "CLI --help"
    else
        fail "CLI --help: unexpected output"
    fi
}

test_cli_monitor_ping() {
    local output
    output=$("${BINARY}" monitor ping 127.0.0.1 2>&1)
    if echo "${output}" | jq -e '.protocol == "icmp" and .target == "127.0.0.1" and .success == true' >/dev/null 2>&1; then
        pass "CLI monitor ping 127.0.0.1"
    else
        fail "CLI monitor ping: unexpected output: ${output}"
    fi
}

test_cli_monitor_tcp_failure() {
    local output
    output=$("${BINARY}" monitor tcp 127.0.0.1 54321 2>&1)
    if echo "${output}" | jq -e '.protocol == "tcp" and .target == "127.0.0.1:54321" and .success == false' >/dev/null 2>&1; then
        pass "CLI monitor tcp 127.0.0.1 54321 (failure)"
    else
        fail "CLI monitor tcp failure: unexpected output: ${output}"
    fi
}

test_cli_discover_local() {
    local output
    output=$("${BINARY}" discover local 2>&1)
    if echo "${output}" | grep -q "Discovered.*interfaces" && echo "${output}" | grep -q "Discovered.*routes"; then
        pass "CLI discover local"
    else
        fail "CLI discover local: unexpected output: ${output}"
    fi
}

# ===== HTTP TESTS =====
test_http_health() {
    assert_status "${BASE_URL}/health" "200" "HTTP GET /health"
    assert_json_field "${BASE_URL}/health" ".status" "ok" "HTTP /health JSON"
}

test_http_interfaces() {
    assert_status "${BASE_URL}/api/v1/interfaces" "200" "HTTP GET /api/v1/interfaces"
    assert_json_field_exists "${BASE_URL}/api/v1/interfaces" ".interfaces" "HTTP /interfaces has interfaces array"
    local count
    count=$(curl -s "${BASE_URL}/api/v1/interfaces" | jq '.interfaces | length' 2>/dev/null || echo "0")
    if [[ "${count}" -gt 0 ]]; then
        pass "HTTP /interfaces: ${count} interfaces found"
    else
        fail "HTTP /interfaces: no interfaces found"
    fi
}

test_http_routes() {
    assert_status "${BASE_URL}/api/v1/routes" "200" "HTTP GET /api/v1/routes"
    assert_json_field_exists "${BASE_URL}/api/v1/routes" ".routes" "HTTP /routes has routes array"
    local count
    count=$(curl -s "${BASE_URL}/api/v1/routes" | jq '.routes | length' 2>/dev/null || echo "0")
    if [[ "${count}" -gt 0 ]]; then
        pass "HTTP /routes: ${count} routes found"
    else
        fail "HTTP /routes: no routes found"
    fi
}

test_http_monitor_ping() {
    local resp
    resp=$(curl -s -X POST "${BASE_URL}/api/v1/monitor/ping" \
        -H "Content-Type: application/json" \
        -d '{"target":"127.0.0.1","timeout_ms":2000}' 2>/dev/null)
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/api/v1/monitor/ping" \
        -H "Content-Type: application/json" \
        -d '{"target":"127.0.0.1","timeout_ms":2000}' 2>/dev/null)
    if [[ "${status}" == "200" ]]; then
        pass "HTTP POST /api/v1/monitor/ping: HTTP 200"
    else
        fail "HTTP POST /api/v1/monitor/ping: expected HTTP 200, got ${status}"
    fi

    if echo "${resp}" | jq -e '.protocol == "icmp" and .target == "127.0.0.1" and .success == true' >/dev/null 2>&1; then
        pass "HTTP /monitor/ping: correct JSON response"
    else
        fail "HTTP /monitor/ping: unexpected JSON: ${resp}"
    fi
}

test_http_monitor_tcp_success() {
    # Start a temporary TCP listener
    local port
    port=$(python3 -c "import socket; s=socket.socket(); s.bind(('',0)); print(s.getsockname()[1]); s.close()")
    python3 -c "
import socket, threading, time
s = socket.socket()
s.bind(('127.0.0.1', ${port}))
s.listen(1)
def accept():
    conn, _ = s.accept()
    conn.close()
    s.close()
t = threading.Thread(target=accept)
t.daemon = True
t.start()
time.sleep(10)
" &
    local python_pid=$!

    sleep 0.5

    local resp
    resp=$(curl -s -X POST "${BASE_URL}/api/v1/monitor/tcp" \
        -H "Content-Type: application/json" \
        -d "{\"host\":\"127.0.0.1\",\"port\":${port},\"timeout_ms\":2000}" 2>/dev/null)
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/api/v1/monitor/tcp" \
        -H "Content-Type: application/json" \
        -d "{\"host\":\"127.0.0.1\",\"port\":${port},\"timeout_ms\":2000}" 2>/dev/null)

    if [[ "${status}" == "200" ]]; then
        pass "HTTP POST /api/v1/monitor/tcp (success): HTTP 200"
    else
        fail "HTTP POST /api/v1/monitor/tcp (success): expected HTTP 200, got ${status}"
    fi

    if echo "${resp}" | jq -e '.protocol == "tcp" and .success == true' >/dev/null 2>&1; then
        pass "HTTP /monitor/tcp (success): correct JSON response"
    else
        fail "HTTP /monitor/tcp (success): unexpected JSON: ${resp}"
    fi

    kill ${python_pid} 2>/dev/null || true
}

test_http_monitor_tcp_failure() {
    local resp
    resp=$(curl -s -X POST "${BASE_URL}/api/v1/monitor/tcp" \
        -H "Content-Type: application/json" \
        -d '{"host":"127.0.0.1","port":54321,"timeout_ms":2000}' 2>/dev/null)
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/api/v1/monitor/tcp" \
        -H "Content-Type: application/json" \
        -d '{"host":"127.0.0.1","port":54321,"timeout_ms":2000}' 2>/dev/null)

    if [[ "${status}" == "200" ]]; then
        pass "HTTP POST /api/v1/monitor/tcp (failure): HTTP 200"
    else
        fail "HTTP POST /api/v1/monitor/tcp (failure): expected HTTP 200, got ${status}"
    fi

    if echo "${resp}" | jq -e '.protocol == "tcp" and .success == false and .error != null' >/dev/null 2>&1; then
        pass "HTTP /monitor/tcp (failure): correct JSON response"
    else
        fail "HTTP /monitor/tcp (failure): unexpected JSON: ${resp}"
    fi
}

test_http_placeholder_endpoints() {
    local endpoints=(
        "/api/v1/vlans"
        "/api/v1/subnets"
        "/api/v1/gateways"
        "/api/v1/events"
        "/api/v1/topology"
    )
    for ep in "${endpoints[@]}"; do
        assert_status "${BASE_URL}${ep}" "200" "HTTP GET ${ep}"
        local resp
        resp=$(curl -s "${BASE_URL}${ep}" 2>/dev/null)
        if echo "${resp}" | jq -e 'type == "array" and length == 0' >/dev/null 2>&1; then
            pass "HTTP ${ep}: empty array"
        else
            fail "HTTP ${ep}: expected empty array, got: ${resp}"
        fi
    done

    # Test /api/v1/devices returns new format
    assert_status "${BASE_URL}/api/v1/devices" "200" "HTTP GET /api/v1/devices"
    local resp
    resp=$(curl -s "${BASE_URL}/api/v1/devices" 2>/dev/null)
    if echo "${resp}" | jq -e 'type == "object" and (.devices | type == "array")' >/dev/null 2>&1; then
        pass "HTTP /api/v1/devices: correct format with devices array"
    else
        fail "HTTP /api/v1/devices: expected object with devices array, got: ${resp}"
    fi
}

test_http_ping_invalid_content_type() {
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/api/v1/monitor/ping" \
        -H "Content-Type: text/plain" \
        -d "target=127.0.0.1" 2>/dev/null)
    if [[ "${status}" == "415" ]]; then
        pass "HTTP /monitor/ping invalid Content-Type: 415"
    else
        fail "HTTP /monitor/ping invalid Content-Type: expected 415, got ${status}"
    fi
}

# ===== METRICS TESTS =====
test_metrics_before_checks() {
    # Restart server to get clean metrics
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
    start_server

    assert_metrics_contains "netwatch_info" "Metrics: netwatch_info present"
    assert_metrics_contains "netwatch_uptime_seconds" "Metrics: netwatch_uptime_seconds present"
    assert_metrics_contains "netwatch_scheduler_jobs_total" "Metrics: netwatch_scheduler_jobs_total present"
    assert_metrics_contains "netwatch_scheduler_errors_total" "Metrics: netwatch_scheduler_errors_total present"
}

test_metrics_after_icmp_check() {
    # Make an ICMP check via HTTP
    curl -s -X POST "${BASE_URL}/api/v1/monitor/ping" \
        -H "Content-Type: application/json" \
        -d '{"target":"127.0.0.1","timeout_ms":2000}' >/dev/null 2>&1

    assert_metrics_contains 'netwatch_health_checks_total{protocol="icmp"}' "Metrics: ICMP total counter"
    assert_metrics_contains 'netwatch_health_check_latency_seconds' "Metrics: ICMP latency histogram"
}

test_metrics_after_tcp_check() {
    # Make a TCP check via HTTP
    local port
    port=$(python3 -c "import socket; s=socket.socket(); s.bind(('',0)); print(s.getsockname()[1]); s.close()")
    python3 -c "
import socket, threading, time
s = socket.socket()
s.bind(('127.0.0.1', ${port}))
s.listen(1)
def accept():
    conn, _ = s.accept()
    conn.close()
    s.close()
t = threading.Thread(target=accept)
t.daemon = True
t.start()
time.sleep(10)
" &
    local python_pid=$!
    sleep 0.5

    curl -s -X POST "${BASE_URL}/api/v1/monitor/tcp" \
        -H "Content-Type: application/json" \
        -d "{\"host\":\"127.0.0.1\",\"port\":${port},\"timeout_ms\":2000}" >/dev/null 2>&1

    assert_metrics_contains 'netwatch_health_checks_total{protocol="tcp"}' "Metrics: TCP total counter"
    assert_metrics_contains 'netwatch_health_check_latency_seconds' "Metrics: TCP latency histogram"

    kill ${python_pid} 2>/dev/null || true
}

test_metrics_cardinality() {
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")

    # Check that health check metrics only have 'protocol' label (and 'le' for histogram)
    local forbidden_labels=("host" "hostname" "ip" "target" "url" "port" "address" "destination" "source" "interface" "device" "instance")

    for metric in "netwatch_health_checks_total" "netwatch_health_check_failures_total" "netwatch_health_check_latency_seconds"; do
        while IFS= read -r line; do
            if [[ "${line}" == \#* ]] || [[ -z "${line}" ]]; then
                continue
            fi
            if [[ "${line}" == ${metric}* ]]; then
                if [[ "${line}" == *{* ]]; then
                    local labels_part
                    labels_part=$(echo "${line}" | sed -n 's/.*{\(.*\)}.*/\1/p')
                    IFS=',' read -ra LABELS <<< "${labels_part}"
                    for label in "${LABELS[@]}"; do
                        local label_name
                        label_name=$(echo "${label}" | cut -d'=' -f1)
                        if [[ "${label_name}" == "le" ]]; then
                            continue  # histogram bucket label is allowed
                        fi
                        local forbidden=0
                        for fl in "${forbidden_labels[@]}"; do
                            if [[ "${label_name}" == "${fl}" ]]; then
                                forbidden=1
                                break
                            fi
                        done
                        if [[ ${forbidden} -eq 1 ]]; then
                            fail "Metrics cardinality: forbidden label '${label_name}' on ${metric}"
                        elif [[ "${label_name}" != "protocol" ]]; then
                            fail "Metrics cardinality: unexpected label '${label_name}' on ${metric} (only 'protocol' allowed)"
                        fi
                    done
                fi
            fi
        done <<< "${body}"
    done
    pass "Metrics cardinality: only 'protocol' label on health check metrics"
}

test_app_metrics_no_labels() {
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")

    for metric in "netwatch_info" "netwatch_uptime_seconds" "netwatch_scheduler_jobs_total" "netwatch_scheduler_errors_total"; do
        while IFS= read -r line; do
            if [[ "${line}" == \#* ]] || [[ -z "${line}" ]]; then
                continue
            fi
            if [[ "${line}" == ${metric}* ]]; then
                if [[ "${line}" == *{* ]]; then
                    fail "AppMetrics: ${metric} should have no labels, found: ${line}"
                fi
            fi
        done <<< "${body}"
    done
    pass "AppMetrics: no labels on internal metrics"
}

# ===== MAIN =====
main() {
    echo "========================================"
    echo "Netwatch E2E Smoke Test"
    echo "========================================"
    echo "Log file: ${LOG_FILE}"
    echo ""

    check_binary
    start_server

    log "=== CLI Tests ==="
    test_cli_version
    test_cli_help
    test_cli_monitor_ping
    test_cli_monitor_tcp_failure
    test_cli_discover_local

    log "=== HTTP Tests ==="
    test_http_health
    test_http_interfaces
    test_http_routes
    test_http_monitor_ping
    test_http_monitor_tcp_success
    test_http_monitor_tcp_failure
    test_http_placeholder_endpoints
    test_http_ping_invalid_content_type

    log "=== Metrics Tests ==="
    test_metrics_before_checks
    test_metrics_after_icmp_check
    test_metrics_after_tcp_check
    test_metrics_cardinality
    test_app_metrics_no_labels

    echo ""
    echo "========================================"
    echo "Results: ${PASS_COUNT} passed, ${FAIL_COUNT} failed"
    echo "========================================"

    if [[ ${FAIL_COUNT} -gt 0 ]]; then
        exit 1
    fi
}

main "$@"