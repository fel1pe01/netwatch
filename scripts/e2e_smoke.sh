#!/usr/bin/env bash
set -uo pipefail

# Netwatch E2E Smoke Test
# Tests the actual release binary: CLI commands, HTTP endpoints, and Prometheus metrics
#
# Variables opcionales:
#   NETWATCH_BIN     Ruta al binario (por defecto $CARGO_TARGET_DIR|target/release/netwatch)
#   E2E_KEEP_LOGS=1  Conserva el directorio temporal (logs) aunque todo pase

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BINARY="${NETWATCH_BIN:-${CARGO_TARGET_DIR:-${PROJECT_ROOT}/target}/release/netwatch}"

# Puerto por defecto del servidor (el binario escucha en 0.0.0.0:8080).
PORT=8080
BASE_URL="http://127.0.0.1:${PORT}"

SERVER_PID=""
LISTENER_PID=""
LISTENER_PORT=""
HTTP_STATUS=""
HTTP_BODY=""

PASS_COUNT=0
FAIL_COUNT=0

# --- dependencias -------------------------------------------------------------
for dep in curl jq python3 ping timeout; do
    if ! command -v "${dep}" >/dev/null 2>&1; then
        echo "Falta la dependencia '${dep}' (necesaria para el E2E)." >&2
        exit 2
    fi
done

# Todas las peticiones HTTP llevan timeout: un servidor colgado no debe colgar el script.
curl() { command curl --max-time 10 "$@"; }

# --- directorio de trabajo aislado ---------------------------------------------
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/netwatch-e2e.XXXXXX")"
LOG_FILE="${WORK_DIR}/e2e.log"
SERVER_LOG="${WORK_DIR}/server.log"

# El binario lee $HOME/.config/netwatch/netwatch.yaml: se aísla para que la
# configuración local del usuario no influya (la CI usa los valores por defecto).
export HOME="${WORK_DIR}/home"
export XDG_CONFIG_HOME="${HOME}/.config"
mkdir -p "${XDG_CONFIG_HOME}"

# Colors for output (solo si la salida es un terminal)
if [[ -t 1 ]]; then
    RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'; NC=$'\033[0m'
else
    RED=""; GREEN=""; YELLOW=""; NC=""
fi

log() {
    echo "${GREEN}[INFO]${NC} $*" | tee -a "${LOG_FILE}"
}

warn() {
    echo "${YELLOW}[WARN]${NC} $*" | tee -a "${LOG_FILE}"
}

fail() {
    echo "${RED}[FAIL]${NC} $*" | tee -a "${LOG_FILE}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
}

pass() {
    echo "${GREEN}[PASS]${NC} $*" | tee -a "${LOG_FILE}"
    PASS_COUNT=$((PASS_COUNT + 1))
}

stop_listener() {
    if [[ -n "${LISTENER_PID}" ]]; then
        kill "${LISTENER_PID}" 2>/dev/null || true
        wait "${LISTENER_PID}" 2>/dev/null || true
        LISTENER_PID=""
    fi
}

stop_server() {
    if [[ -n "${SERVER_PID}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
        kill "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
    SERVER_PID=""
}

cleanup() {
    stop_listener
    if [[ -n "${SERVER_PID}" ]]; then
        log "Stopping server (PID: ${SERVER_PID})"
        stop_server
    fi
    if [[ "${FAIL_COUNT}" -eq 0 && -z "${E2E_KEEP_LOGS:-}" ]]; then
        rm -rf "${WORK_DIR}"
    else
        echo "Logs conservados en: ${WORK_DIR}"
    fi
}

# INT/TERM deben terminar el script (si no, seguiría ejecutando tests sin servidor).
trap 'exit 130' INT
trap 'exit 143' TERM
trap cleanup EXIT

check_binary() {
    if [[ ! -x "${BINARY}" ]]; then
        fail "Binary not found or not executable at ${BINARY}. Run 'cargo build --release' first."
        exit 1
    fi
    log "Using binary: ${BINARY}"
}

# Ejecuta el binario con timeout para que un comando colgado no bloquee el E2E.
run_bin() {
    timeout 20 "${BINARY}" "$@"
}

start_server() {
    log "Starting netwatch server..."

    # Si ya hay algo escuchando, el health check de abajo daría un falso positivo
    # contra otro proceso (p. ej. una instancia de desarrollo).
    if curl -s -o /dev/null --max-time 1 "${BASE_URL}/health" 2>/dev/null; then
        fail "Port ${PORT} already in use (another netwatch running?). Stop it and retry."
        exit 1
    fi

    # Use a temporary database file for each test run
    local test_db="${WORK_DIR}/netwatch_${RANDOM}.sqlite"
    NETWATCH_STORAGE_PATH="${test_db}" "${BINARY}" >>"${SERVER_LOG}" 2>&1 &
    SERVER_PID=$!

    # Wait for server to be ready (hasta ~20 s: los runners de CI pueden ser lentos)
    local max_attempts=100
    local attempt=0
    while [[ ${attempt} -lt ${max_attempts} ]]; do
        if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
            fail "Server exited during startup"
            cat "${SERVER_LOG}"
            SERVER_PID=""
            exit 1
        fi
        if curl -s -f "${BASE_URL}/health" >/dev/null 2>&1; then
            log "Server started on ${BASE_URL} (PID: ${SERVER_PID})"
            return 0
        fi
        sleep 0.2
        attempt=$((attempt + 1))
    done

    fail "Server failed to start within ${max_attempts} attempts"
    cat "${SERVER_LOG}"
    exit 1
}

# Listener TCP de un solo proceso: elige puerto libre, lo publica en un fichero y
# acepta conexiones hasta que se le mata. Deja LISTENER_PID y LISTENER_PORT.
start_listener() {
    local port_file="${WORK_DIR}/listener.port"
    rm -f "${port_file}"
    python3 - "${port_file}" <<'PY' &
import socket, sys
s = socket.socket()
s.bind(("127.0.0.1", 0))
s.listen(16)
with open(sys.argv[1], "w") as f:
    f.write(str(s.getsockname()[1]))
s.settimeout(60)
try:
    while True:
        conn, _ = s.accept()
        conn.close()
except Exception:
    pass
PY
    LISTENER_PID=$!

    local i=0
    while [[ ! -s "${port_file}" && ${i} -lt 50 ]]; do
        sleep 0.1
        i=$((i + 1))
    done
    LISTENER_PORT="$(cat "${port_file}" 2>/dev/null || true)"
    if [[ -z "${LISTENER_PORT}" ]]; then
        fail "Could not start local TCP listener"
        return 1
    fi
}

# POST JSON una sola vez; deja el cuerpo en HTTP_BODY y el código en HTTP_STATUS.
http_post() {
    local url="$1"
    local payload="$2"
    local out
    out=$(curl -s -w $'\n%{http_code}' -X POST "${url}" \
        -H "Content-Type: application/json" \
        -d "${payload}" 2>/dev/null) || out=$'\n000'
    HTTP_STATUS="${out##*$'\n'}"
    HTTP_BODY="${out%$'\n'*}"
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

# Test helper: assert metrics contains string (literal)
assert_metrics_contains() {
    local pattern="$1"
    local desc="$2"
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
    if grep -qF -- "${pattern}" <<< "${body}"; then
        pass "${desc}: found '${pattern}'"
    else
        fail "${desc}: pattern '${pattern}' not found in metrics"
    fi
}

# Test helper: assert metrics does NOT contain string (literal)
assert_metrics_not_contains() {
    local pattern="$1"
    local desc="$2"
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
    if ! grep -qF -- "${pattern}" <<< "${body}"; then
        pass "${desc}: correctly absent '${pattern}'"
    else
        fail "${desc}: pattern '${pattern}' unexpectedly found in metrics"
    fi
}

# ===== CLI TESTS =====
test_cli_version() {
    local output
    output=$(run_bin --version 2>&1)
    if grep -q "netwatch" <<< "${output}"; then
        pass "CLI --version"
    else
        fail "CLI --version: unexpected output: ${output}"
    fi
}

test_cli_help() {
    local output
    output=$(run_bin --help 2>&1)
    if grep -q "Netwatch network monitoring tool" <<< "${output}"; then
        pass "CLI --help"
    else
        fail "CLI --help: unexpected output"
    fi
}

test_cli_monitor_ping() {
    local output
    output=$(run_bin monitor ping 127.0.0.1 2>&1)
    if jq -e '.protocol == "icmp" and .target == "127.0.0.1" and .success == true' >/dev/null 2>&1 <<< "${output}"; then
        pass "CLI monitor ping 127.0.0.1"
    else
        fail "CLI monitor ping: unexpected output: ${output}"
    fi
}

test_cli_monitor_tcp_failure() {
    local output
    output=$(run_bin monitor tcp 127.0.0.1 54321 2>&1)
    if jq -e '.protocol == "tcp" and .target == "127.0.0.1:54321" and .success == false' >/dev/null 2>&1 <<< "${output}"; then
        pass "CLI monitor tcp 127.0.0.1 54321 (failure)"
    else
        fail "CLI monitor tcp failure: unexpected output: ${output}"
    fi
}

test_cli_discover_local() {
    local output
    output=$(run_bin discover local 2>&1)
    if grep -q "Discovered.*interfaces" <<< "${output}" && grep -q "Discovered.*routes" <<< "${output}"; then
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
    http_post "${BASE_URL}/api/v1/monitor/ping" '{"target":"127.0.0.1","timeout_ms":2000}'
    if [[ "${HTTP_STATUS}" == "200" ]]; then
        pass "HTTP POST /api/v1/monitor/ping: HTTP 200"
    else
        fail "HTTP POST /api/v1/monitor/ping: expected HTTP 200, got ${HTTP_STATUS}"
    fi

    if jq -e '.protocol == "icmp" and .target == "127.0.0.1" and .success == true' >/dev/null 2>&1 <<< "${HTTP_BODY}"; then
        pass "HTTP /monitor/ping: correct JSON response"
    else
        fail "HTTP /monitor/ping: unexpected JSON: ${HTTP_BODY}"
    fi
}

test_http_monitor_tcp_success() {
    if ! start_listener; then
        return
    fi

    http_post "${BASE_URL}/api/v1/monitor/tcp" \
        "{\"host\":\"127.0.0.1\",\"port\":${LISTENER_PORT},\"timeout_ms\":2000}"

    if [[ "${HTTP_STATUS}" == "200" ]]; then
        pass "HTTP POST /api/v1/monitor/tcp (success): HTTP 200"
    else
        fail "HTTP POST /api/v1/monitor/tcp (success): expected HTTP 200, got ${HTTP_STATUS}"
    fi

    if jq -e '.protocol == "tcp" and .success == true' >/dev/null 2>&1 <<< "${HTTP_BODY}"; then
        pass "HTTP /monitor/tcp (success): correct JSON response"
    else
        fail "HTTP /monitor/tcp (success): unexpected JSON: ${HTTP_BODY}"
    fi

    stop_listener
}

test_http_monitor_tcp_failure() {
    http_post "${BASE_URL}/api/v1/monitor/tcp" '{"host":"127.0.0.1","port":54321,"timeout_ms":2000}'

    if [[ "${HTTP_STATUS}" == "200" ]]; then
        pass "HTTP POST /api/v1/monitor/tcp (failure): HTTP 200"
    else
        fail "HTTP POST /api/v1/monitor/tcp (failure): expected HTTP 200, got ${HTTP_STATUS}"
    fi

    if jq -e '.protocol == "tcp" and .success == false and .error != null' >/dev/null 2>&1 <<< "${HTTP_BODY}"; then
        pass "HTTP /monitor/tcp (failure): correct JSON response"
    else
        fail "HTTP /monitor/tcp (failure): unexpected JSON: ${HTTP_BODY}"
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
    local ep resp
    for ep in "${endpoints[@]}"; do
        assert_status "${BASE_URL}${ep}" "200" "HTTP GET ${ep}"
        resp=$(curl -s "${BASE_URL}${ep}" 2>/dev/null)
        if jq -e 'type == "array" and length == 0' >/dev/null 2>&1 <<< "${resp}"; then
            pass "HTTP ${ep}: empty array"
        else
            fail "HTTP ${ep}: expected empty array, got: ${resp}"
        fi
    done

    # Test /api/v1/devices returns new format
    assert_status "${BASE_URL}/api/v1/devices" "200" "HTTP GET /api/v1/devices"
    resp=$(curl -s "${BASE_URL}/api/v1/devices" 2>/dev/null)
    if jq -e 'type == "object" and (.devices | type == "array")' >/dev/null 2>&1 <<< "${resp}"; then
        pass "HTTP /api/v1/devices: correct format with devices array"
    else
        fail "HTTP /api/v1/devices: expected object with devices array, got: ${resp}"
    fi
}

test_http_ping_invalid_content_type() {
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/api/v1/monitor/ping" \
        -H "Content-Type: text/plain" \
        -d "target=127.0.0.1" 2>/dev/null || echo "000")
    if [[ "${status}" == "415" ]]; then
        pass "HTTP /monitor/ping invalid Content-Type: 415"
    else
        fail "HTTP /monitor/ping invalid Content-Type: expected 415, got ${status}"
    fi
}

# ===== METRICS TESTS =====
test_metrics_before_checks() {
    # Restart server to get clean metrics
    stop_server
    start_server

    assert_metrics_contains "netwatch_info" "Metrics: netwatch_info present"
    assert_metrics_contains "netwatch_uptime_seconds" "Metrics: netwatch_uptime_seconds present"
    assert_metrics_contains "netwatch_scheduler_jobs_total" "Metrics: netwatch_scheduler_jobs_total present"
    assert_metrics_contains "netwatch_scheduler_errors_total" "Metrics: netwatch_scheduler_errors_total present"
}

test_metrics_after_icmp_check() {
    # Make an ICMP check via HTTP
    http_post "${BASE_URL}/api/v1/monitor/ping" '{"target":"127.0.0.1","timeout_ms":2000}'

    assert_metrics_contains 'netwatch_health_checks_total{protocol="icmp"}' "Metrics: ICMP total counter"
    assert_metrics_contains 'netwatch_health_check_latency_seconds' "Metrics: ICMP latency histogram"
}

test_metrics_after_tcp_check() {
    # Make a TCP check via HTTP
    if ! start_listener; then
        return
    fi

    http_post "${BASE_URL}/api/v1/monitor/tcp" \
        "{\"host\":\"127.0.0.1\",\"port\":${LISTENER_PORT},\"timeout_ms\":2000}"

    assert_metrics_contains 'netwatch_health_checks_total{protocol="tcp"}' "Metrics: TCP total counter"
    assert_metrics_contains 'netwatch_health_check_latency_seconds' "Metrics: TCP latency histogram"

    stop_listener
}

test_metrics_cardinality() {
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
    local problems=0

    # Check that health check metrics only have 'protocol' label (and 'le' for histogram)
    local forbidden_labels=("host" "hostname" "ip" "target" "url" "port" "address" "destination" "source" "interface" "device" "instance")

    local metric line labels_part label label_name fl forbidden
    local LABELS
    for metric in "netwatch_health_checks_total" "netwatch_health_check_failures_total" "netwatch_health_check_latency_seconds"; do
        while IFS= read -r line; do
            if [[ "${line}" == \#* ]] || [[ -z "${line}" ]]; then
                continue
            fi
            if [[ "${line}" == ${metric}* ]]; then
                if [[ "${line}" == *"{"* ]]; then
                    labels_part=$(echo "${line}" | sed -n 's/.*{\(.*\)}.*/\1/p')
                    IFS=',' read -ra LABELS <<< "${labels_part}"
                    for label in "${LABELS[@]}"; do
                        label_name=$(echo "${label}" | cut -d'=' -f1)
                        if [[ "${label_name}" == "le" ]]; then
                            continue  # histogram bucket label is allowed
                        fi
                        forbidden=0
                        for fl in "${forbidden_labels[@]}"; do
                            if [[ "${label_name}" == "${fl}" ]]; then
                                forbidden=1
                                break
                            fi
                        done
                        if [[ ${forbidden} -eq 1 ]]; then
                            fail "Metrics cardinality: forbidden label '${label_name}' on ${metric}"
                            problems=$((problems + 1))
                        elif [[ "${label_name}" != "protocol" ]]; then
                            fail "Metrics cardinality: unexpected label '${label_name}' on ${metric} (only 'protocol' allowed)"
                            problems=$((problems + 1))
                        fi
                    done
                fi
            fi
        done <<< "${body}"
    done
    if [[ ${problems} -eq 0 ]]; then
        pass "Metrics cardinality: only 'protocol' label on health check metrics"
    fi
}

test_app_metrics_no_labels() {
    local body
    body=$(curl -s "${BASE_URL}/metrics" 2>/dev/null || echo "")
    local problems=0
    local metric line

    for metric in "netwatch_info" "netwatch_uptime_seconds" "netwatch_scheduler_jobs_total" "netwatch_scheduler_errors_total"; do
        while IFS= read -r line; do
            if [[ "${line}" == \#* ]] || [[ -z "${line}" ]]; then
                continue
            fi
            if [[ "${line}" == ${metric}* ]]; then
                if [[ "${line}" == *"{"* ]]; then
                    fail "AppMetrics: ${metric} should have no labels, found: ${line}"
                    problems=$((problems + 1))
                fi
            fi
        done <<< "${body}"
    done
    if [[ ${problems} -eq 0 ]]; then
        pass "AppMetrics: no labels on internal metrics"
    fi
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
        echo ""
        echo "--- últimas líneas del log del servidor (${SERVER_LOG}) ---"
        tail -n 50 "${SERVER_LOG}" 2>/dev/null || true
        exit 1
    fi
}

main "$@"