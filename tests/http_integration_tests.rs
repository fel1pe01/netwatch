use netwatch::api::{router, AppState};
use netwatch::auth::AuthState;
use netwatch::config::Config;
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

async fn spawn_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let auth_state = Arc::new(AuthState::new(netwatch::auth::AuthConfig {
        enabled: false,
        tokens: HashMap::new(),
        public_paths: vec!["/health".to_string(), "/metrics".to_string()],
        token_header: "Authorization".to_string(),
    }));

    // Use permissive SSRF config for testing (allow loopback/private IPs)
    let test_ssrf_config = SsrfConfig {
        allow_loopback: true,
        allow_private_ips: true,
        ..Default::default()
    };
    let ssrf_state = Arc::new(SsrfState::new(test_ssrf_config).unwrap());
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

    let config = Arc::new(Config::default());
    let app_state = AppState::new(storage, ssrf_state, auth_state, rate_limiter, config);

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
    (base_url, handle)
}

#[tokio::test]
async fn http_health_endpoint() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

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
async fn http_interfaces_endpoint() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    // New format: { items, total, limit, offset }
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());

    let interfaces = body["items"].as_array().unwrap();
    // Note: DB may be empty initially, so we don't assert non-empty
    // but we verify the structure is correct

    if !interfaces.is_empty() {
        let iface = &interfaces[0];
        assert!(iface["id"].is_string());
        assert!(iface["device_id"].is_string());
        assert!(iface["name"].is_string() || iface["name"].is_null());
        assert!(iface["mac"].is_string() || iface["mac"].is_null());
        assert!(iface["mtu"].is_number() || iface["mtu"].is_null());
        assert!(iface["state"].is_string() || iface["state"].is_null());
        assert!(iface["created_at"].is_string());
    }
}

#[tokio::test]
async fn http_routes_endpoint() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/api/v1/routes", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    // New format: { items, total, limit, offset }
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());

    let routes = body["items"].as_array().unwrap();
    // Note: DB may be empty initially

    if !routes.is_empty() {
        let route = &routes[0];
        assert!(route["id"].is_string());
        assert!(route["destination"].is_string());
        assert!(route["gateway"].is_string() || route["gateway"].is_null());
        assert!(route["interface"].is_string());
        assert!(route["metric"].is_number() || route["metric"].is_null());
        assert!(route["family"].is_string());
        assert!(route["created_at"].is_string());
    }
}

#[tokio::test]
async fn http_hosts_endpoint() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    // New format: { items, total, limit, offset }
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());

    let hosts = body["items"].as_array().unwrap();
    // Should have at least the local device
    assert!(!hosts.is_empty(), "Should have at least local device");

    let host = &hosts[0];
    assert!(host["id"].is_string());
    assert!(host["hostname"].is_string() || host["hostname"].is_null());
    assert!(host["ip"].is_string() || host["ip"].is_null());
    assert!(host["device_type"].is_string() || host["device_type"].is_null());
    assert!(host["created_at"].is_string());
}

#[tokio::test]
async fn http_monitor_ping_endpoint() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = json!({"target": "127.0.0.1", "timeout_ms": 2000});
    let resp = client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["protocol"], "icmp");
    assert_eq!(body["target"], "127.0.0.1");
    assert!(body["success"].as_bool().unwrap_or(false));
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["evidence"], "icmp");
    assert_eq!(body["timeout_ms"], 2000);
}

#[tokio::test]
async fn http_monitor_tcp_endpoint_success() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
    });

    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = json!({"host": "127.0.0.1", "port": port, "timeout_ms": 2000});
    let resp = client
        .post(format!("{}/api/v1/monitor/tcp", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["protocol"], "tcp");
    assert_eq!(body["target"], format!("127.0.0.1:{}", port));
    assert!(body["success"].as_bool().unwrap_or(false));
    assert!(body["latency_ms"].is_number());
    assert_eq!(body["evidence"], "tcp_probe");
    assert_eq!(body["timeout_ms"], 2000);
}

#[tokio::test]
async fn http_monitor_tcp_endpoint_failure() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = json!({"host": "127.0.0.1", "port": 54321, "timeout_ms": 2000});
    let resp = client
        .post(format!("{}/api/v1/monitor/tcp", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["protocol"], "tcp");
    assert_eq!(body["target"], "127.0.0.1:54321");
    assert_eq!(body["success"], false);
    assert!(body["latency_ms"].is_null());
    assert!(body["error"].is_string());
    assert_eq!(body["evidence"], "tcp_probe");
    assert_eq!(body["timeout_ms"], 2000);
}

#[tokio::test]
async fn http_metrics_endpoint_before_checks() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    // AppMetrics (netwatch_info, netwatch_uptime_seconds) must always be present
    assert!(body.contains("netwatch_info"));
    assert!(body.contains("netwatch_uptime_seconds"));
    // Scheduler metrics (netwatch_scheduler_jobs_total, netwatch_scheduler_errors_total)
    // are CounterVec with 'job' label and only appear when scheduler jobs run.
    // They are not part of AppMetrics anymore.
    // Health check metrics may be present from other tests due to shared registry.
}

#[tokio::test]
async fn http_metrics_after_icmp_check() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = json!({"target": "127.0.0.1", "timeout_ms": 2000});
    client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(body.contains("netwatch_health_checks_total{protocol=\"icmp\"}"));
    assert!(body.contains("netwatch_health_check_latency_seconds"));
    // Failure metric only appears on failed checks; this check succeeds
}

#[tokio::test]
async fn http_metrics_after_tcp_check() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
    });

    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = json!({"host": "127.0.0.1", "port": port, "timeout_ms": 2000});
    client
        .post(format!("{}/api/v1/monitor/tcp", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.text().await.unwrap();
    assert!(body.contains("netwatch_health_checks_total{protocol=\"tcp\"}"));
    assert!(body.contains("netwatch_health_check_latency_seconds"));
}

#[tokio::test]
async fn http_placeholder_endpoints() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    // These endpoints return empty arrays
    let array_endpoints = [
        "/api/v1/vlans",
        "/api/v1/subnets",
        "/api/v1/gateways",
        "/api/v1/topology",
    ];

    for endpoint in array_endpoints {
        let resp = client
            .get(format!("{}{}", base_url, endpoint))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    // Events endpoint returns paginated response object with items array
    let resp = client
        .get(format!("{}/api/v1/events", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body.get("items").is_some());
    assert!(body["items"].is_array());
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert!(body.get("limit").is_some());
    assert!(body.get("offset").is_some());
    assert!(body.get("total").is_some());
}

#[tokio::test]
async fn http_ping_invalid_content_type() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .header("Content-Type", "text/plain")
        .body("target=127.0.0.1")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 415);
}

#[tokio::test]
async fn http_metrics_after_icmp_failure() {
    // Restart server to get clean metrics
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    // Make an ICMP check that will fail (unreachable target)
    let req = json!({"target": "10.255.255.1", "timeout_ms": 1000});
    let resp = client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    // The ping should fail (unreachable)
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["protocol"], "icmp");
    assert_eq!(body["success"], false);
    assert!(body["error"].is_string());

    // Check metrics
    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let metrics_body = resp.text().await.unwrap();
    assert!(metrics_body.contains("netwatch_health_checks_total{protocol=\"icmp\"}"));
    assert!(metrics_body.contains("netwatch_health_check_failures_total{protocol=\"icmp\"}"));
}

#[tokio::test]
async fn http_metrics_after_tcp_failure() {
    // Restart server to get clean metrics
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    // Make a TCP check that will fail (closed port)
    let req = json!({"host": "127.0.0.1", "port": 54321, "timeout_ms": 1000});
    let resp = client
        .post(format!("{}/api/v1/monitor/tcp", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    // The TCP check should fail (connection refused)
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["protocol"], "tcp");
    assert_eq!(body["success"], false);
    assert!(body["error"].is_string());

    // Check metrics
    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let metrics_body = resp.text().await.unwrap();
    assert!(metrics_body.contains("netwatch_health_checks_total{protocol=\"tcp\"}"));
    assert!(metrics_body.contains("netwatch_health_check_failures_total{protocol=\"tcp\"}"));
}
