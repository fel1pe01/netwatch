use crate::auth::{extract_bearer_token, AuthState};
use crate::auth::{AuthUser, Role};
use crate::capacity_planning::{CapacityConfig, CapacityPlanner};
use crate::config::Config;
use crate::incidents::{confirm_root_cause, IncidentStore};
use crate::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use crate::monitoring::validation::validation_error_result;
use crate::monitoring::HealthCheckResult;
use crate::rate_limit::{rate_limit_middleware, RateLimitState};
use crate::root_cause::RootCauseAnalyzer;
use crate::ssrf::SsrfState;
use crate::storage::{
    get_check_results, get_devices, get_events, get_interfaces, get_routes, CheckResult,
    CheckResultsFilter, DeviceRow, DevicesFilter, EventV2, EventsFilter, InterfaceRow,
    InterfacesFilter, PaginatedResponse, RouteRow, RoutesFilter, Storage,
};
use chrono::{DateTime, Utc};

/// Uniform error response for all API errors
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Device response with optional MAC based on role
#[derive(Serialize)]
pub struct DeviceResponse {
    pub id: String,
    pub hostname: Option<String>,
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    pub device_type: Option<String>,
    pub source: Option<String>,
    pub confidence: Option<String>,
    pub observed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Interface response with optional MAC based on role
#[derive(Serialize)]
pub struct InterfaceResponse {
    pub id: String,
    pub device_id: String,
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    pub mtu: Option<i64>,
    pub state: Option<String>,
    pub source: Option<String>,
    pub confidence: Option<String>,
    pub observed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_seen: Option<String>,
    /// Whether the interface is considered present (not absent)
    pub present: bool,
}

fn filter_device_row(row: DeviceRow, role: &Role) -> DeviceResponse {
    let show_mac = matches!(role, Role::Administrator | Role::Operator);
    DeviceResponse {
        id: row.id,
        hostname: row.hostname,
        ip: row.ip,
        mac: if show_mac { row.mac } else { None },
        device_type: row.device_type,
        source: row.source,
        confidence: row.confidence,
        observed_at: row.observed_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn filter_interface_row(
    row: InterfaceRow,
    role: &Role,
    absence_threshold_seconds: i64,
) -> InterfaceResponse {
    let show_mac = matches!(role, Role::Administrator | Role::Operator);
    // Compute present based on last_seen and absence threshold
    let present = row
        .last_seen
        .as_ref()
        .and_then(|ls| ls.parse::<DateTime<Utc>>().ok())
        .map(|last_seen| {
            last_seen > (Utc::now() - chrono::Duration::seconds(absence_threshold_seconds))
        })
        .unwrap_or(false);
    InterfaceResponse {
        id: row.id,
        device_id: row.device_id,
        name: row.name,
        mac: if show_mac { row.mac } else { None },
        mtu: row.mtu,
        state: row.state,
        source: row.source,
        confidence: row.confidence,
        observed_at: row.observed_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_seen: row.last_seen,
        present,
    }
}
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::{
    extract::{Path, Request, State},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;
use tracing::warn;

/// Combined application state for all handlers
#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    pub ssrf_state: Arc<SsrfState>,
    pub auth_state: Arc<AuthState>,
    pub rate_limiter: RateLimitState,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(
        storage: Arc<Storage>,
        ssrf_state: Arc<SsrfState>,
        auth_state: Arc<AuthState>,
        rate_limiter: RateLimitState,
        config: Arc<Config>,
    ) -> Self {
        Self {
            storage,
            ssrf_state,
            auth_state,
            rate_limiter,
            config,
        }
    }
}

pub async fn health_handler() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

// Query parameters for inventory endpoints
#[derive(Deserialize)]
pub struct InventoryQueryParams {
    // Pagination
    limit: Option<i64>,
    offset: Option<i64>,
    // Ordering (whitelisted)
    order_by: Option<String>,
    order_dir: Option<String>,
    // Exact-match filters only
    id: Option<String>,
    ip: Option<String>,
    hostname: Option<String>,
    device_type: Option<String>,
    name: Option<String>,
    destination: Option<String>,
    interface: Option<String>,
    gateway: Option<String>,
    // Presence filter (exact match: true|false)
    present: Option<String>,
}

/// Devices handler with pagination - returns { items, total, limit, offset }
pub async fn devices_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<InventoryQueryParams>,
    request: Request,
) -> Result<Json<PaginatedResponse<DeviceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Validate limit and offset - return 400 for invalid values
    let limit = params.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "limit must be between 1 and 500".to_string(),
            }),
        ));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "offset must be non-negative".to_string(),
            }),
        ));
    }

    // Get user role from request extensions (set by auth middleware)
    let role = request
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.role) // Role implements Copy
        .unwrap_or(Role::Viewer);

    let filter = DevicesFilter {
        id: params.id,
        ip: params.ip,
        hostname: params.hostname,
        device_type: params.device_type,
        limit,
        offset,
        order_by: params.order_by,
        order_dir: params.order_dir,
    };

    let result = get_devices(&state.storage.pool, filter)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get devices: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
        })?;

    let filtered = PaginatedResponse {
        items: result
            .items
            .into_iter()
            .map(|r| filter_device_row(r, &role))
            .collect(),
        total: result.total,
        limit: result.limit,
        offset: result.offset,
    };

    Ok(Json(filtered))
}

/// Empty handler for placeholder endpoints
async fn empty_handler() -> Json<Value> {
    Json(json!([]))
}

/// Alias for devices_handler - GET /api/v1/hosts
pub async fn hosts_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<InventoryQueryParams>,
    request: Request,
) -> Result<Json<PaginatedResponse<DeviceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    devices_handler(State(state), axum::extract::Query(params), request).await
}

/// Interfaces handler with pagination - reads from DB, returns { items, total, limit, offset }
pub async fn interfaces_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<InventoryQueryParams>,
    request: Request,
) -> Result<Json<PaginatedResponse<InterfaceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Validate limit and offset - return 400 for invalid values
    let limit = params.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "limit must be between 1 and 500".to_string(),
            }),
        ));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "offset must be non-negative".to_string(),
            }),
        ));
    }

    // Validate present filter
    let present = match params.present.as_deref() {
        Some("true") => Some(true),
        Some("false") => Some(false),
        Some(v) => {
            // Truncate value in error message to 32 chars to avoid log injection
            let display_value = if v.len() > 32 {
                format!("{}...", &v[..29])
            } else {
                v.to_string()
            };
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid present filter value: {}", display_value),
                }),
            ));
        }
        None => None,
    };

    // Get user role from request extensions (set by auth middleware)
    let role = request
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.role) // Role implements Copy
        .unwrap_or(Role::Viewer);

    // Get absence threshold from config (discovery interval * absence_multiplier)
    let absence_threshold =
        state.config.discovery.interval * state.config.discovery.absence_multiplier;

    let filter = InterfacesFilter {
        device_id: params.id, // reuse id param for device_id filter
        name: params.name,
        present,
        limit,
        offset,
        order_by: params.order_by,
        order_dir: params.order_dir,
    };

    let result = get_interfaces(&state.storage.pool, filter, absence_threshold as i64)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get interfaces: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
        })?;

    let filtered = PaginatedResponse {
        items: result
            .items
            .into_iter()
            .map(|r| filter_interface_row(r, &role, absence_threshold as i64))
            .collect(),
        total: result.total,
        limit: result.limit,
        offset: result.offset,
    };

    Ok(Json(filtered))
}

/// Routes handler with pagination - reads from DB, returns { items, total, limit, offset }
pub async fn routes_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<InventoryQueryParams>,
    _request: Request,
) -> Result<Json<PaginatedResponse<RouteRow>>, (StatusCode, Json<ErrorResponse>)> {
    // Validate limit and offset - return 400 for invalid values
    let limit = params.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "limit must be between 1 and 500".to_string(),
            }),
        ));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "offset must be non-negative".to_string(),
            }),
        ));
    }

    // Validate present filter
    let present = match params.present.as_deref() {
        Some("true") => Some(true),
        Some("false") => Some(false),
        Some(v) => {
            // Truncate value in error message to 32 chars to avoid log injection
            let display_value = if v.len() > 32 {
                format!("{}...", &v[..29])
            } else {
                v.to_string()
            };
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid present filter value: {}", display_value),
                }),
            ));
        }
        None => None,
    };

    // Get absence threshold from config (discovery interval * absence_multiplier)
    let absence_threshold =
        state.config.discovery.interval * state.config.discovery.absence_multiplier;

    let filter = RoutesFilter {
        destination: params.destination,
        interface: params.interface,
        gateway: params.gateway,
        present,
        limit,
        offset,
        order_by: params.order_by,
        order_dir: params.order_dir,
    };

    let result = get_routes(&state.storage.pool, filter, absence_threshold as i64)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get routes: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            )
        })?;

    let filtered = PaginatedResponse {
        items: result
            .items
            .into_iter()
            .map(|mut r| {
                // Compute present based on last_seen and absence threshold
                let threshold = Utc::now() - chrono::Duration::seconds(absence_threshold as i64);
                r.present = r
                    .last_seen
                    .parse::<DateTime<Utc>>()
                    .map(|last_seen| last_seen > threshold)
                    .unwrap_or(false);
                r
            })
            .collect(),
        total: result.total,
        limit: result.limit,
        offset: result.offset,
    };

    Ok(Json(filtered))
}

#[derive(Deserialize)]
pub struct PingRequest {
    target: String,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    2000
}

pub async fn ping_handler(
    State(state): State<AppState>,
    Json(req): Json<PingRequest>,
) -> Json<HealthCheckResult> {
    let policy = state.ssrf_state.policy();
    let res = crate::monitoring::icmp::ping_with_ssrf(policy, &req.target, req.timeout_ms)
        .await
        .unwrap_or_else(|e| validation_error_result(&req.target, "icmp", req.timeout_ms, e));
    // Update metrics
    let proto = res.protocol.as_str();
    HEALTH_CHECK_TOTAL.with_label_values(&[proto]).inc();
    if !res.success {
        HEALTH_CHECK_FAILURES_TOTAL
            .with_label_values(&[proto])
            .inc();
    }
    if let Some(latency) = res.latency_ms {
        let latency_seconds = latency as f64 / 1000.0;
        HEALTH_CHECK_LATENCY_SECONDS
            .with_label_values(&[proto])
            .observe(latency_seconds);
    }
    Json(res)
}

#[derive(Deserialize)]
pub struct TcpRequest {
    host: String,
    port: u16,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

pub async fn tcp_handler(
    State(state): State<AppState>,
    Json(req): Json<TcpRequest>,
) -> Json<HealthCheckResult> {
    let policy = state.ssrf_state.policy();
    let res =
        crate::monitoring::tcp::check_tcp_with_ssrf(policy, &req.host, req.port, req.timeout_ms)
            .await
            .unwrap_or_else(|e| {
                validation_error_result(
                    &format!("{}:{}", req.host, req.port),
                    "tcp",
                    req.timeout_ms,
                    e,
                )
            });
    // Update metrics
    let proto = res.protocol.as_str();
    HEALTH_CHECK_TOTAL.with_label_values(&[proto]).inc();
    if !res.success {
        HEALTH_CHECK_FAILURES_TOTAL
            .with_label_values(&[proto])
            .inc();
    }
    if let Some(latency) = res.latency_ms {
        let latency_seconds = latency as f64 / 1000.0;
        HEALTH_CHECK_LATENCY_SECONDS
            .with_label_values(&[proto])
            .observe(latency_seconds);
    }
    Json(res)
}

#[derive(Deserialize)]
pub struct ConfirmHypothesisRequest {
    hypothesis_id: String,
    confirmed: bool,
    actor: String,
}

pub async fn hypotheses_handler(State(state): State<AppState>) -> Json<Value> {
    let incident_store = IncidentStore::new(state.storage.pool.clone());
    let _incidents = incident_store.list_open().await.unwrap_or_default();
    let topology_store = crate::topology_store::TopologyStore::new(state.storage.pool.clone());
    let analyzer = RootCauseAnalyzer::new(Arc::new(topology_store), state.storage.clone());
    let hypotheses = analyzer
        .analyze(chrono::Utc::now() - chrono::Duration::hours(24), None)
        .await
        .unwrap_or_default();
    Json(json!({"hypotheses": hypotheses}))
}

pub async fn hypothesis_detail_handler(
    State(state): State<AppState>,
    Path(hypothesis_id): Path<String>,
) -> Json<Value> {
    use crate::incidents::IncidentStore;
    let incident_store = IncidentStore::new(state.storage.pool.clone());
    match incident_store.find_by_hypothesis_id(&hypothesis_id).await {
        Ok(Some(incident)) => {
            let hypothesis = crate::root_cause::Hypothesis {
                id: incident.root_cause_hypothesis_id.unwrap_or_default(),
                entity_id: incident.entity_id,
                entity_type: incident.entity_type,
                entity_label: incident.title,
                confidence: 0.7,
                evidence: vec![],
                dependency_path: vec![],
                time_window_start: incident.created_at,
                time_window_end: incident.updated_at,
                created_at: incident.created_at,
                confirmed: incident.root_cause_confirmed,
                confirmed_by: incident.root_cause_confirmed_by,
                confirmed_at: incident.root_cause_confirmed_at,
                is_root_cause: incident.root_cause_confirmed,
            };
            Json(json!(hypothesis))
        }
        Ok(None) => Json(json!({"error": "hypothesis not found"})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

pub async fn confirm_hypothesis_handler(
    State(state): State<AppState>,
    Path(incident_id): Path<String>,
    Json(req): Json<ConfirmHypothesisRequest>,
) -> Json<Value> {
    use crate::incidents::IncidentStore;
    let incident_store = IncidentStore::new(state.storage.pool.clone());
    match confirm_root_cause(
        &incident_store,
        &incident_id,
        &req.hypothesis_id,
        req.confirmed,
        req.actor,
    )
    .await
    {
        Ok(_) => Json(json!({"status": "ok", "message": "hypothesis confirmation recorded"})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct CapacityReportRequest {
    #[serde(default = "default_markdown")]
    pub format: String,
}

fn default_markdown() -> String {
    "markdown".to_string()
}

/// Capacity planning report handler
pub async fn capacity_report_handler(
    State(state): State<AppState>,
    Json(req): Json<CapacityReportRequest>,
) -> Json<Value> {
    let config = CapacityConfig::default();
    let mut planner = CapacityPlanner::new(state.storage.clone(), config);
    match planner.analyze_capacity().await {
        Ok(report) => match req.format.as_str() {
            "json" => match report.to_json() {
                Ok(json_str) => Json(
                    serde_json::from_str(&json_str)
                        .unwrap_or(json!({"error": "Failed to parse JSON"})),
                ),
                Err(e) => Json(json!({"error": e.to_string()})),
            },
            _ => Json(json!({"report": report.to_markdown()})),
        },
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

/// Alertmanager webhook payload
#[derive(Debug, Deserialize, Serialize)]
pub struct AlertmanagerPayload {
    pub version: String,
    pub group_key: String,
    pub status: String,
    pub receiver: String,
    pub group_labels: std::collections::HashMap<String, String>,
    pub common_labels: std::collections::HashMap<String, String>,
    pub common_annotations: std::collections::HashMap<String, String>,
    pub external_url: String,
    pub alerts: Vec<AlertmanagerAlert>,
}

/// Alertmanager alert
#[derive(Debug, Deserialize, Serialize)]
pub struct AlertmanagerAlert {
    pub status: String,
    pub labels: std::collections::HashMap<String, String>,
    pub annotations: std::collections::HashMap<String, String>,
    pub starts_at: String,
    pub ends_at: String,
    pub generator_url: String,
    pub fingerprint: String,
}

/// Alertmanager webhook handler
pub async fn alertmanager_webhook_handler(
    State(_state): State<AppState>,
    Json(payload): Json<AlertmanagerPayload>,
) -> Json<Value> {
    tracing::info!(
        "Received Alertmanager webhook: status={}, alerts={}, receiver={}",
        payload.status,
        payload.alerts.len(),
        payload.receiver
    );

    for alert in &payload.alerts {
        tracing::info!(
            "Alert: {} status={} labels={:?} annotations={:?}",
            alert.fingerprint,
            alert.status,
            alert.labels,
            alert.annotations
        );
    }

    Json(json!({"status": "ok"}))
}

/// Query parameters for checks and events pagination
#[derive(Deserialize)]
pub struct PaginationParams {
    limit: Option<i64>,
    offset: Option<i64>,
    entity_id: Option<String>,
    entity_type: Option<String>,
    protocol: Option<String>,
    success: Option<bool>,
    since: Option<String>,
    until: Option<String>,
    event_type: Option<String>,
}

/// Checks handler with pagination - returns { items, total, limit, offset }
pub async fn checks_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<CheckResult>>, (StatusCode, Json<ErrorResponse>)> {
    // Validate limit and offset - return 400 for invalid values
    let limit = params.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "limit must be between 1 and 500".to_string(),
            }),
        ));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "offset must be non-negative".to_string(),
            }),
        ));
    }

    let since = params.since.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let until = params.until.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let filter = CheckResultsFilter {
        entity_id: params.entity_id,
        entity_type: params.entity_type,
        protocol: params.protocol,
        success: params.success,
        since,
        until,
        limit,
        offset,
    };

    match get_check_results(&state.storage.pool, filter).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            tracing::error!("Failed to get checks: {}", e);
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            ))
        }
    }
}

/// Events handler with pagination - returns { items, total, limit, offset }
pub async fn events_handler(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<EventV2>>, (StatusCode, Json<ErrorResponse>)> {
    // Validate limit and offset - return 400 for invalid values
    let limit = params.limit.unwrap_or(50);
    if !(1..=500).contains(&limit) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "limit must be between 1 and 500".to_string(),
            }),
        ));
    }
    let offset = params.offset.unwrap_or(0);
    if offset < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "offset must be non-negative".to_string(),
            }),
        ));
    }

    let since = params.since.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let until = params.until.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let filter = EventsFilter {
        entity_id: params.entity_id,
        entity_type: params.entity_type,
        event_type: params.event_type,
        since,
        until,
        limit,
        offset,
    };

    match get_events(&state.storage.pool, filter).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            tracing::error!("Failed to get events: {}", e);
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                }),
            ))
        }
    }
}

pub fn router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/v1/devices", get(devices_handler))
        .route("/api/v1/hosts", get(hosts_handler))
        .route("/api/v1/vlans", get(empty_handler))
        .route("/api/v1/subnets", get(empty_handler))
        .route("/api/v1/gateways", get(empty_handler))
        .route("/api/v1/events", get(events_handler))
        .route("/api/v1/checks", get(checks_handler))
        .route("/api/v1/interfaces", get(interfaces_handler))
        .route("/api/v1/routes", get(routes_handler))
        .route("/api/v1/monitor/ping", post(ping_handler))
        .route("/api/v1/monitor/tcp", post(tcp_handler))
        .route("/api/v1/topology", get(empty_handler))
        .route("/api/v1/incidents/hypotheses", get(hypotheses_handler))
        .route(
            "/api/v1/incidents/hypotheses/{hypothesis_id}",
            get(hypothesis_detail_handler),
        )
        .route(
            "/api/v1/incidents/{incident_id}/hypothesis/confirm",
            post(confirm_hypothesis_handler),
        )
        .route("/api/v1/capacity/report", post(capacity_report_handler))
        .route("/api/v1/alerts/webhook", post(alertmanager_webhook_handler))
        .layer(axum::middleware::from_fn_with_state(
            app_state.rate_limiter.clone(),
            rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            app_state.auth_state.clone(),
            auth_middleware,
        ))
        .with_state(app_state)
}

async fn auth_middleware(
    State(auth_state): State<Arc<AuthState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if !auth_state.is_enabled().await {
        return Ok(next.run(request).await);
    }
    let path = request.uri().path();
    if auth_state.is_public_path(path).await {
        return Ok(next.run(request).await);
    }
    let header_name = auth_state.token_header().await;
    let token = extract_bearer_token(request.headers(), &header_name).ok_or_else(|| {
        warn!("Missing or invalid Authorization header for path: {}", path);
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;
    let user = auth_state.validate_token(&token).await.ok_or_else(|| {
        warn!("Invalid token for path: {}", path);
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
            }),
        )
    })?;
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}
