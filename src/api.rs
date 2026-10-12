use crate::discovery::interfaces::discover_interfaces;
use crate::discovery::routing::discover_routes;
use crate::incidents::{confirm_root_cause, IncidentStore};
use crate::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use crate::monitoring::{check_tcp, ping, HealthCheckResult};
use crate::root_cause::RootCauseAnalyzer;
use crate::storage::Storage;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
use std::sync::Arc;

pub async fn health_handler() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

pub async fn devices_handler(State(storage): State<Arc<Storage>>) -> Json<Value> {
    use serde::{Deserialize, Serialize};
    use sqlx::FromRow;

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    struct DeviceRow {
        id: String,
        hostname: Option<String>,
        ip: Option<String>,
        mac: Option<String>,
        device_type: Option<String>,
        source: Option<String>,
        confidence: Option<String>,
        observed_at: Option<String>,
        created_at: String,
        updated_at: String,
    }

    let devices =
        sqlx::query_as::<_, DeviceRow>("SELECT * FROM devices ORDER BY created_at DESC LIMIT 100")
            .fetch_all(&storage.pool)
            .await
            .unwrap_or_default();

    Json(json!({"devices": devices}))
}

async fn empty_handler() -> Json<Value> {
    Json(json!([]))
}

pub async fn interfaces_handler() -> Json<Value> {
    let interfaces = discover_interfaces().await.unwrap_or_default();
    Json(json!({"interfaces": interfaces}))
}

pub async fn routes_handler() -> Json<Value> {
    let routes = discover_routes().await.unwrap_or_default();
    Json(json!({"routes": routes}))
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

pub async fn ping_handler(Json(req): Json<PingRequest>) -> Json<HealthCheckResult> {
    let res = ping(&req.target, req.timeout_ms)
        .await
        .unwrap_or_else(|e| HealthCheckResult {
            target: req.target,
            protocol: "icmp".to_string(),
            timestamp: chrono::Utc::now(),
            success: false,
            latency_ms: None,
            error: Some(e.to_string()),
            observed_at: chrono::Utc::now(),
            timeout_ms: req.timeout_ms,
            evidence: "icmp".to_string(),
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
pub struct TcpRequest {
    host: String,
    port: u16,
    #[serde(default = "default_timeout_ms")]
    timeout_ms: u64,
}

pub async fn tcp_handler(Json(req): Json<TcpRequest>) -> Json<HealthCheckResult> {
    let res = check_tcp(&req.host, req.port, req.timeout_ms)
        .await
        .unwrap_or_else(|e| HealthCheckResult {
            target: format!("{}:{}", req.host, req.port),
            protocol: "tcp".to_string(),
            timestamp: chrono::Utc::now(),
            success: false,
            latency_ms: None,
            error: Some(e.to_string()),
            observed_at: chrono::Utc::now(),
            timeout_ms: req.timeout_ms,
            evidence: "tcp_probe".to_string(),
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

pub async fn hypotheses_handler(State(storage): State<Arc<Storage>>) -> Json<Value> {
    let incident_store = IncidentStore::new(storage.pool.clone());
    let _incidents = incident_store.list_open().await.unwrap_or_default();
    let topology_store = crate::topology_store::TopologyStore::new(storage.pool.clone());
    let analyzer = RootCauseAnalyzer::new(Arc::new(topology_store), storage.clone());
    let hypotheses = analyzer
        .analyze(chrono::Utc::now() - chrono::Duration::hours(24), None)
        .await
        .unwrap_or_default();
    Json(json!({"hypotheses": hypotheses}))
}

pub async fn hypothesis_detail_handler(
    State(storage): State<Arc<Storage>>,
    Path(hypothesis_id): Path<String>,
) -> Json<Value> {
    use crate::incidents::IncidentStore;
    let incident_store = IncidentStore::new(storage.pool.clone());
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
            };
            Json(json!(hypothesis))
        }
        Ok(None) => Json(json!({"error": "hypothesis not found"})),
        Err(e) => Json(json!({"error": e.to_string()})),
    }
}

pub async fn confirm_hypothesis_handler(
    State(storage): State<Arc<Storage>>,
    Path(incident_id): Path<String>,
    Json(req): Json<ConfirmHypothesisRequest>,
) -> Json<Value> {
    use crate::incidents::IncidentStore;
    let incident_store = IncidentStore::new(storage.pool.clone());
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

pub fn router(storage: Arc<Storage>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/api/v1/devices", get(devices_handler))
        .route("/api/v1/vlans", get(empty_handler))
        .route("/api/v1/subnets", get(empty_handler))
        .route("/api/v1/gateways", get(empty_handler))
        .route("/api/v1/events", get(empty_handler))
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
        .with_state(storage)
}
