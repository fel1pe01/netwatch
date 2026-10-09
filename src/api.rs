use crate::discovery::interfaces::discover_interfaces;
use crate::discovery::routing::discover_routes;
use crate::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use crate::monitoring::{check_tcp, ping, HealthCheckResult};
use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;
pub async fn health_handler() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

pub async fn devices_handler() -> Json<Value> {
    Json(json!([]))
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

pub fn router() -> Router {
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
}
