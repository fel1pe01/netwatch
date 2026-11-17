use netwatch::api::{router, AppState};
use netwatch::auth::AuthState;
use netwatch::metrics::AppMetrics;
use netwatch::rate_limit::{RateLimitConfig, RateLimiter};
use netwatch::ssrf::{SsrfConfig, SsrfState};
use netwatch::storage::Storage;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::net::TcpListener;

/// Spawn a test server with DEFAULT config (auth enabled, SSRF strict, no allow_loopback/allow_private_ips)
async fn spawn_default_config_server() -> (String, tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    // Create a valid token
    let mut tokens = HashMap::new();
    let test_token = "test-token-12345";
    tokens.insert(
        test_token.to_string(),
        netwatch::auth::AuthUser::new("testuser".to_string(), netwatch::auth::Role::Operator),
    );

    let auth_state = Arc::new(AuthState::new(netwatch::auth::AuthConfig {
        enabled: true,
        tokens,
        public_paths: vec!["/health".to_string(), "/metrics".to_string()],
        token_header: "Authorization".to_string(),
    }));

    // Use DEFAULT SSRF config (strict - no allow_loopback, no allow_private_ips)
    let ssrf_state = Arc::new(SsrfState::new(SsrfConfig::default()).unwrap());
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

    let app_state = AppState::new(storage, ssrf_state, auth_state, rate_limiter);

    let app_metrics = AppMetrics::new();
    let uptime = app_metrics.uptime.clone();
    let start_time = app_metrics.start_time;
    let app = router(app_state).merge(axum::Router::new().route(
        "/metrics",
        axum::routing::get(move || AppMetrics::metrics_handler(uptime.clone(), start_time)),
    ));

    let handle = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    (base_url, handle, test_token.to_string())
}

#[tokio::test]
async fn default_config_health_endpoint_public() {
    let (base_url, _handle, _token) = spawn_default_config_server().await;
    let client = Client::new();

    // /health should be public (no auth required)
    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn default_config_metrics_endpoint_public() {
    let (base_url, _handle, _token) = spawn_default_config_server().await;
    let client = Client::new();

    // /metrics should be public (no auth required)
    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("netwatch_info"));
}

#[tokio::test]
async fn default_config_protected_endpoint_requires_auth() {
    let (base_url, _handle, _token) = spawn_default_config_server().await;
    let client = Client::new();

    // /api/v1/interfaces should require auth
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn default_config_protected_endpoint_with_valid_token() {
    let (base_url, _handle, token) = spawn_default_config_server().await;
    let client = Client::new();

    // With valid token, should succeed
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["interfaces"].is_array());
}

#[tokio::test]
async fn default_config_protected_endpoint_with_invalid_token() {
    let (base_url, _handle, _token) = spawn_default_config_server().await;
    let client = Client::new();

    // With invalid token, should fail
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .header("Authorization", "Bearer invalid-token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn default_config_ssrf_blocks_loopback() {
    let (base_url, _handle, token) = spawn_default_config_server().await;
    let client = Client::new();

    // POST to /monitor/ping with loopback target should be blocked by SSRF
    let resp = client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&json!({"target": "127.0.0.1", "timeout_ms": 2000}))
        .send()
        .await
        .unwrap();

    // Should return 200 but with SSRF validation error in response
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"]
        .as_str()
        .unwrap_or("")
        .contains("SSRF validation failed"));
    assert!(body["evidence"] == "ssrf_validation");
}

#[tokio::test]
async fn default_config_ssrf_blocks_private_ips() {
    let (base_url, _handle, token) = spawn_default_config_server().await;
    let client = Client::new();

    // POST to /monitor/ping with private IP target should be blocked by SSRF
    let resp = client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&json!({"target": "192.168.1.1", "timeout_ms": 2000}))
        .send()
        .await
        .unwrap();

    // Should return 200 but with SSRF validation error in response
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"]
        .as_str()
        .unwrap_or("")
        .contains("SSRF validation failed"));
    assert!(body["evidence"] == "ssrf_validation");
}

#[tokio::test]
async fn default_config_ssrf_allows_public_ips() {
    let (base_url, _handle, token) = spawn_default_config_server().await;
    let client = Client::new();

    // POST to /monitor/ping with public IP target should be allowed (but will fail connection)
    // Note: This will fail due to connection, not SSRF
    let resp = client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&json!({"target": "8.8.8.8", "timeout_ms": 2000}))
        .send()
        .await
        .unwrap();

    // Should return 200 - connection may succeed or fail, but not SSRF blocked
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // Not blocked by SSRF (evidence should be "icmp" not "ssrf_validation")
    // Success depends on network, but error should not be SSRF validation
    if !body["success"].as_bool().unwrap_or(false) {
        assert!(!body["error"]
            .as_str()
            .unwrap_or("")
            .contains("SSRF validation failed"));
    }
}
