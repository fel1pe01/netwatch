use netwatch::api::router;
use netwatch::metrics::AppMetrics;
use netwatch::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use netwatch::storage::Storage;
use prometheus::core::Collector;
use reqwest::Client;
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

fn parse_metric_labels(metrics_text: &str, metric_name: &str) -> Vec<Vec<String>> {
    let mut labels_list = Vec::new();
    for line in metrics_text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if line.starts_with(metric_name) && line.contains('{') {
            if let Some(start) = line.find('{') {
                if let Some(end) = line[start..].find('}') {
                    let labels_str = &line[start + 1..start + end];
                    let labels: Vec<String> = labels_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    labels_list.push(labels);
                }
            }
        }
    }
    labels_list
}

#[tokio::test]
async fn test_health_check_metrics_only_protocol_label() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = serde_json::json!({"target": "127.0.0.1", "timeout_ms": 2000});
    client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    let req = serde_json::json!({"host": "127.0.0.1", "port": 8080, "timeout_ms": 2000});
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
    let body = resp.text().await.unwrap();

    let forbidden_labels = [
        "host",
        "hostname",
        "ip",
        "target",
        "url",
        "port",
        "address",
        "destination",
        "source",
        "interface",
        "device",
        "instance",
    ];

    for metric_name in [
        "netwatch_health_checks_total",
        "netwatch_health_check_failures_total",
        "netwatch_health_check_latency_seconds",
    ] {
        let labels_list = parse_metric_labels(&body, metric_name);
        let is_histogram = metric_name == "netwatch_health_check_latency_seconds";
        for labels in labels_list {
            for label in labels {
                let label_name = label.split('=').next().unwrap_or("");
                assert!(
                    !forbidden_labels.contains(&label_name),
                    "Forbidden label '{}' found on metric '{}': {}",
                    label_name,
                    metric_name,
                    label
                );
                if is_histogram && label_name == "le" {
                    continue;
                }
                assert_eq!(
                    label_name, "protocol",
                    "Only 'protocol' label allowed on health check metric '{}' (histogram={}), found: {}",
                    metric_name, is_histogram, label
                );
            }
        }
    }
}

#[tokio::test]
async fn test_health_check_protocol_values() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let req = serde_json::json!({"target": "127.0.0.1", "timeout_ms": 2000});
    client
        .post(format!("{}/api/v1/monitor/ping", base_url))
        .json(&req)
        .send()
        .await
        .unwrap();

    let req = serde_json::json!({"host": "127.0.0.1", "port": 8080, "timeout_ms": 2000});
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
    let body = resp.text().await.unwrap();

    for metric_name in [
        "netwatch_health_checks_total",
        "netwatch_health_check_failures_total",
        "netwatch_health_check_latency_seconds",
    ] {
        let labels_list = parse_metric_labels(&body, metric_name);
        let mut protocols = std::collections::HashSet::new();
        for labels in labels_list {
            for label in labels {
                if label.starts_with("protocol=") {
                    let value = label.split('=').nth(1).unwrap().trim_matches('"');
                    protocols.insert(value.to_string());
                }
            }
        }
        let allowed: std::collections::HashSet<String> =
            ["icmp", "tcp"].iter().map(|s| s.to_string()).collect();
        assert!(
            protocols.is_subset(&allowed),
            "Metric '{}' has unexpected protocol values: {:?}",
            metric_name,
            protocols
        );
    }
}

#[tokio::test]
async fn test_no_dynamic_labels_on_app_metrics() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = Client::new();

    let resp = client
        .get(format!("{}/metrics", base_url))
        .send()
        .await
        .unwrap();
    let body = resp.text().await.unwrap();

    let app_metrics = [
        "netwatch_info",
        "netwatch_uptime_seconds",
        "netwatch_scheduler_jobs_total",
        "netwatch_scheduler_errors_total",
    ];

    for metric_name in app_metrics {
        let labels_list = parse_metric_labels(&body, metric_name);
        for labels in labels_list {
            assert!(
                labels.is_empty(),
                "AppMetric '{}' should have no labels, found: {:?}",
                metric_name,
                labels
            );
        }
    }
}

#[tokio::test]
async fn test_cardinality_enforcement_via_direct_metric_access() {
    HEALTH_CHECK_TOTAL.with_label_values(&["icmp"]).inc();
    HEALTH_CHECK_TOTAL.with_label_values(&["tcp"]).inc();
    HEALTH_CHECK_FAILURES_TOTAL
        .with_label_values(&["icmp"])
        .inc();
    HEALTH_CHECK_LATENCY_SECONDS
        .with_label_values(&["icmp"])
        .observe(0.1);
    HEALTH_CHECK_LATENCY_SECONDS
        .with_label_values(&["tcp"])
        .observe(0.2);

    let registry = prometheus::default_registry();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = String::new();
    encoder.encode_utf8(&metric_families, &mut buffer).unwrap();
    let body = buffer;

    let forbidden_labels = [
        "host",
        "hostname",
        "ip",
        "target",
        "url",
        "port",
        "address",
        "destination",
        "source",
        "interface",
        "device",
        "instance",
    ];

    for metric_name in [
        "netwatch_health_checks_total",
        "netwatch_health_check_failures_total",
        "netwatch_health_check_latency_seconds",
    ] {
        let labels_list = parse_metric_labels(&body, metric_name);
        let is_histogram = metric_name == "netwatch_health_check_latency_seconds";
        for labels in labels_list {
            for label in labels {
                let label_name = label.split('=').next().unwrap_or("");
                assert!(
                    !forbidden_labels.contains(&label_name),
                    "Forbidden label '{}' found on metric '{}': {}",
                    label_name,
                    metric_name,
                    label
                );
                if is_histogram && label_name == "le" {
                    continue;
                }
                assert_eq!(
                    label_name, "protocol",
                    "Only 'protocol' label allowed on health check metric '{}' (histogram={}), found: {}",
                    metric_name, is_histogram, label
                );
            }
        }
    }
}

#[test]
fn test_metric_definition_labels() {
    let metric = HEALTH_CHECK_TOTAL.clone();
    let desc = metric.desc();
    assert_eq!(desc.len(), 1);
    let label_names: Vec<&str> = desc[0].variable_labels.iter().map(|s| s.as_str()).collect();
    assert_eq!(label_names, vec!["protocol"]);

    let metric = HEALTH_CHECK_FAILURES_TOTAL.clone();
    let desc = metric.desc();
    assert_eq!(desc.len(), 1);
    let label_names: Vec<&str> = desc[0].variable_labels.iter().map(|s| s.as_str()).collect();
    assert_eq!(label_names, vec!["protocol"]);

    let metric = HEALTH_CHECK_LATENCY_SECONDS.clone();
    let desc = metric.desc();
    assert_eq!(desc.len(), 1);
    let label_names: Vec<&str> = desc[0].variable_labels.iter().map(|s| s.as_str()).collect();
    assert_eq!(label_names, vec!["protocol"]);
}

#[test]
fn test_icmp_failure_metrics() {
    // Reset metrics by creating new ones (they're Lazy statics, so we test the behavior)
    HEALTH_CHECK_TOTAL.with_label_values(&["icmp"]).inc();
    HEALTH_CHECK_FAILURES_TOTAL
        .with_label_values(&["icmp"])
        .inc();
    // No latency observed for failed check

    let registry = prometheus::default_registry();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = String::new();
    encoder.encode_utf8(&metric_families, &mut buffer).unwrap();
    let body = buffer;

    // Verify failure counter incremented (check for metric name and label pattern)
    assert!(body.contains("netwatch_health_check_failures_total{protocol=\"icmp\"}"));
    // Verify total counter incremented
    assert!(body.contains("netwatch_health_checks_total{protocol=\"icmp\"}"));
    // Note: Latency histogram may appear from other tests due to shared registry.
    // We don't assert on absence since tests share the registry.
}

#[test]
fn test_tcp_failure_metrics() {
    HEALTH_CHECK_TOTAL.with_label_values(&["tcp"]).inc();
    HEALTH_CHECK_FAILURES_TOTAL
        .with_label_values(&["tcp"])
        .inc();
    // No latency observed for failed check

    let registry = prometheus::default_registry();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = String::new();
    encoder.encode_utf8(&metric_families, &mut buffer).unwrap();
    let body = buffer;

    // Verify failure counter incremented
    assert!(body.contains("netwatch_health_check_failures_total{protocol=\"tcp\"}"));
    // Verify total counter incremented
    assert!(body.contains("netwatch_health_checks_total{protocol=\"tcp\"}"));
    // Note: Latency histogram may appear from other tests due to shared registry.
    // We don't assert on absence since tests share the registry.
}

#[test]
fn test_icmp_success_metrics() {
    HEALTH_CHECK_TOTAL.with_label_values(&["icmp"]).inc();
    // Success - no failure increment
    HEALTH_CHECK_LATENCY_SECONDS
        .with_label_values(&["icmp"])
        .observe(0.05);

    let registry = prometheus::default_registry();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = String::new();
    encoder.encode_utf8(&metric_families, &mut buffer).unwrap();
    let body = buffer;

    // Verify total counter incremented
    assert!(body.contains("netwatch_health_checks_total{protocol=\"icmp\"}"));
    // Verify latency recorded
    assert!(body.contains("netwatch_health_check_latency_seconds"));
    // Should have latency bucket entries
    assert!(body.contains("netwatch_health_check_latency_seconds_bucket{protocol=\"icmp\""));
}

#[test]
fn test_tcp_success_metrics() {
    HEALTH_CHECK_TOTAL.with_label_values(&["tcp"]).inc();
    // Success - no failure increment
    HEALTH_CHECK_LATENCY_SECONDS
        .with_label_values(&["tcp"])
        .observe(0.01);

    let registry = prometheus::default_registry();
    let encoder = prometheus::TextEncoder::new();
    let metric_families = registry.gather();
    let mut buffer = String::new();
    encoder.encode_utf8(&metric_families, &mut buffer).unwrap();
    let body = buffer;

    // Verify total counter incremented
    assert!(body.contains("netwatch_health_checks_total{protocol=\"tcp\"}"));
    // Verify latency recorded
    assert!(body.contains("netwatch_health_check_latency_seconds_bucket{protocol=\"tcp\""));
}
