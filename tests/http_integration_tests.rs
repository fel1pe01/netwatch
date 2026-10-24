use netwatch::api::router;
use netwatch::metrics::AppMetrics;
use netwatch::storage::Storage;
use reqwest::Client;
use serde_json::{json, Value};
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

    let app_metrics = AppMetrics::new();
    let uptime = app_metrics.uptime.clone();
    let start_time = app_metrics.start_time;
    let app = router(storage).merge(axum::Router::new().route(
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
    assert!(body["interfaces"].is_array());
    let interfaces = body["interfaces"].as_array().unwrap();
    assert!(
        !interfaces.is_empty(),
        "Should have at least loopback interface"
    );

    let iface = &interfaces[0];
    assert!(iface["name"].is_string());
    assert!(iface["ipv4"].is_string() || iface["ipv4"].is_null());
    assert!(iface["evidence"].is_string());
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
    assert!(body["routes"].is_array());
    let routes = body["routes"].as_array().unwrap();
    assert!(!routes.is_empty(), "Should have at least default route");

    let route = &routes[0];
    assert!(route["destination"].is_string());
    assert!(route["interface"].is_string());
    assert!(route["evidence"].is_string());
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

    let endpoints = [
        "/api/v1/vlans",
        "/api/v1/subnets",
        "/api/v1/gateways",
        "/api/v1/events",
        "/api/v1/topology",
    ];

    for endpoint in endpoints {
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
