use netwatch::api::{router, AppState};
use netwatch::auth::{AuthState, AuthUser, Role};
use netwatch::config::Config;
use netwatch::metrics::AppMetrics;
use netwatch::rate_limit::{RateLimitConfig, RateLimiter};
use netwatch::ssrf::{SsrfConfig, SsrfState};
use netwatch::storage::Storage;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::net::TcpListener;

async fn spawn_test_server(auth_enabled: bool) -> (String, tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let mut tokens = HashMap::new();
    tokens.insert(
        "test-token-12345".to_string(),
        AuthUser::new("testuser".to_string(), Role::Administrator),
    );

    let auth_state = Arc::new(AuthState::new(netwatch::auth::AuthConfig {
        enabled: auth_enabled,
        tokens,
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

    // Wait for server to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    (base_url, handle, "test-token-12345".to_string())
}

fn make_auth_headers(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers
}

#[tokio::test]
async fn inventory_without_token_returns_401() {
    let (base_url, handle, _token) = spawn_test_server(true).await;
    let client = Client::new();

    // Test /devices without token
    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Test /hosts without token
    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Test /interfaces without token
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Test /routes without token
    let resp = client
        .get(format!("{}/api/v1/routes", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    handle.abort();
}

#[tokio::test]
async fn inventory_with_valid_token_returns_200_and_correct_format() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    // Test /devices
    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());
    assert_eq!(body["limit"], 50); // default
    assert_eq!(body["offset"], 0); // default

    // Test /hosts (alias)
    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());

    // Test /interfaces
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());

    // Test /routes
    let resp = client
        .get(format!("{}/api/v1/routes", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
    assert!(body["limit"].is_number());
    assert!(body["offset"].is_number());

    handle.abort();
}

#[tokio::test]
async fn inventory_limit_too_high_returns_400() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    let resp = client
        .get(format!("{}/api/v1/devices?limit=1000000", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "limit=1000000 should return 400");

    handle.abort();
}

#[tokio::test]
async fn inventory_limit_zero_returns_400() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    let resp = client
        .get(format!("{}/api/v1/devices?limit=0", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "limit=0 should return 400");

    handle.abort();
}

#[tokio::test]
async fn inventory_limit_negative_returns_400() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    let resp = client
        .get(format!("{}/api/v1/devices?limit=-1", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "limit=-1 should return 400");

    handle.abort();
}

#[tokio::test]
async fn inventory_offset_negative_returns_400() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    let resp = client
        .get(format!("{}/api/v1/devices?offset=-1", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "offset=-1 should return 400");

    handle.abort();
}

#[tokio::test]
async fn inventory_limit_non_numeric_returns_400() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    let resp = client
        .get(format!("{}/api/v1/devices?limit=abc", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "limit=abc should return 400");

    handle.abort();
}

#[tokio::test]
async fn inventory_sql_injection_filter_is_harmless() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    // Try SQL injection in various filter params using reqwest's query method
    let injection = "' OR 1=1 --";

    let test_params = vec![
        ("ip", injection),
        ("hostname", injection),
        ("device_type", injection),
        ("name", injection),
        ("destination", injection),
        ("interface", injection),
        ("gateway", injection),
    ];

    for (key, value) in test_params {
        let resp = client
            .get(format!("{}/api/v1/devices", base_url))
            .query(&[(key, value)])
            .headers(headers.clone())
            .send()
            .await
            .unwrap();

        // Should not return 500 (no internal error leakage)
        // Should return 200 (filter just doesn't match) or 400 (validation)
        assert_ne!(
            resp.status(),
            500,
            "SQL injection should not cause 500 for param: {}={}",
            key,
            value
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_empty_db_returns_200_with_empty_items() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    // Fresh server with empty DB - but local device is created on connect
    // So devices will have 1 item (local), interfaces/routes may be empty
    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    assert!(body["items"].is_array());
    // local device exists
    assert_eq!(body["total"], 1);

    // For interfaces and routes, they may be empty initially
    for endpoint in ["/api/v1/interfaces", "/api/v1/routes"] {
        let resp = client
            .get(format!("{}{}", base_url, endpoint))
            .headers(headers.clone())
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "{} should return 200", endpoint);

        let body: Value = resp.json().await.unwrap();
        assert!(
            body["items"].is_array(),
            "{} should have items array",
            endpoint
        );
        // total can be 0 or more depending on discovery
        assert!(
            body["total"].is_number(),
            "{} total should be a number",
            endpoint
        );
    }

    // /hosts is alias of /devices
    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert!(body["items"].is_array());
    assert_eq!(body["total"], 1);

    handle.abort();
}

#[tokio::test]
async fn inventory_ordering_is_stable() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    // Make two identical requests with explicit ordering
    let resp1 = client
        .get(format!(
            "{}/api/v1/devices?order_by=created_at&order_dir=DESC&limit=10",
            base_url
        ))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), 200);
    let body1: Value = resp1.json().await.unwrap();

    let resp2 = client
        .get(format!(
            "{}/api/v1/devices?order_by=created_at&order_dir=DESC&limit=10",
            base_url
        ))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), 200);
    let body2: Value = resp2.json().await.unwrap();

    // Order should be stable - same IDs in same order
    let ids1: Vec<String> = body1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();
    let ids2: Vec<String> = body2["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();

    assert_eq!(ids1, ids2, "Ordering should be stable between requests");

    handle.abort();
}

#[tokio::test]
async fn inventory_no_sensitive_fields_in_response() {
    let (base_url, handle, token) = spawn_test_server(true).await;
    let client = Client::new();
    let headers = make_auth_headers(&token);

    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();

    // Check that no item contains sensitive fields
    for item in items {
        // These fields should NOT be present in the response
        assert!(
            item.get("password").is_none(),
            "password should not be in response"
        );
        assert!(
            item.get("secret").is_none(),
            "secret should not be in response"
        );
        assert!(
            item.get("token").is_none(),
            "token should not be in response"
        );
        assert!(
            item.get("api_key").is_none(),
            "api_key should not be in response"
        );
        assert!(
            item.get("private_key").is_none(),
            "private_key should not be in response"
        );
    }

    // Same check for interfaces
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    for item in body["items"].as_array().unwrap() {
        assert!(item.get("password").is_none());
        assert!(item.get("secret").is_none());
        assert!(item.get("token").is_none());
    }

    // Same check for routes
    let resp = client
        .get(format!("{}/api/v1/routes", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    for item in body["items"].as_array().unwrap() {
        assert!(item.get("password").is_none());
        assert!(item.get("secret").is_none());
        assert!(item.get("token").is_none());
    }

    handle.abort();
}

/// Spawns a test server with a closed DB pool to simulate DB failure
async fn spawn_test_server_with_closed_db() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    // Create a storage and immediately close its pool
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    storage.pool.close().await;

    let auth_state = Arc::new(AuthState::new(netwatch::auth::AuthConfig {
        enabled: true,
        tokens: {
            let mut m = HashMap::new();
            m.insert(
                "test-token-12345".to_string(),
                AuthUser::new("testuser".to_string(), Role::Administrator),
            );
            m
        },
        public_paths: vec!["/health".to_string(), "/metrics".to_string()],
        token_header: "Authorization".to_string(),
    }));

    let test_ssrf_config = SsrfConfig {
        allow_loopback: true,
        allow_private_ips: true,
        ..Default::default()
    };
    let ssrf_state = Arc::new(SsrfState::new(test_ssrf_config).unwrap());
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));

    let config = Arc::new(Config::default());
    let app_state = AppState::new(
        Arc::new(storage),
        ssrf_state,
        auth_state,
        rate_limiter,
        config,
    );

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

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    (base_url, handle)
}

#[tokio::test]
async fn inventory_db_failure_returns_503_generic() {
    // This test spawns a server with a CLOSED DB pool to simulate DB failure
    // and verifies the response is 503 with a generic JSON error message
    let (base_url, handle) = spawn_test_server_with_closed_db().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-12345");

    // Test /devices with valid token -> 503 JSON
    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        503,
        "/devices should return 503 on DB failure"
    );
    assert_eq!(
        resp.headers().get(reqwest::header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap_or("");
    // Error should be generic, not leak internal details
    assert!(
        !error_msg.contains("sqlite"),
        "error should not mention sqlite: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("SQL"),
        "error should not mention SQL: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("constraint"),
        "error should not mention constraint: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("UNIQUE"),
        "error should not mention UNIQUE: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("foreign key"),
        "error should not mention foreign key: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("closed"),
        "error should not mention pool closed: {}",
        error_msg
    );
    assert!(
        error_msg.contains("Internal server error") || error_msg.contains("internal"),
        "error should be generic: {}",
        error_msg
    );

    // Test /interfaces
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        503,
        "/interfaces should return 503 on DB failure"
    );
    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        !error_msg.contains("sqlite"),
        "error should not mention sqlite: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("SQL"),
        "error should not mention SQL: {}",
        error_msg
    );

    // Test /routes
    let resp = client
        .get(format!("{}/api/v1/routes", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        503,
        "/routes should return 503 on DB failure"
    );
    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap_or("");
    assert!(
        !error_msg.contains("sqlite"),
        "error should not mention sqlite: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("SQL"),
        "error should not mention SQL: {}",
        error_msg
    );

    // Test /devices WITHOUT token -> 401 (auth checked before DB)
    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        401,
        "/devices without token should return 401 even with closed DB"
    );
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Unauthorized");

    handle.abort();
}

/// Spawns a test server with all three roles for MAC visibility testing
async fn spawn_test_server_with_roles() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let mut tokens = HashMap::new();
    tokens.insert(
        "test-token-admin".to_string(),
        AuthUser::new("admin".to_string(), Role::Administrator),
    );
    tokens.insert(
        "test-token-operator".to_string(),
        AuthUser::new("operator".to_string(), Role::Operator),
    );
    tokens.insert(
        "test-token-viewer".to_string(),
        AuthUser::new("viewer".to_string(), Role::Viewer),
    );

    let auth_state = Arc::new(AuthState::new(netwatch::auth::AuthConfig {
        enabled: true,
        tokens,
        public_paths: vec!["/health".to_string(), "/metrics".to_string()],
        token_header: "Authorization".to_string(),
    }));

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

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    (base_url, handle)
}

#[tokio::test]
async fn inventory_mac_visible_for_admin_on_devices() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-admin");

    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty(), "should have at least local device");

    // Admin should see mac field
    for item in items {
        assert!(
            item.get("mac").is_some(),
            "Admin should see mac field: {:?}",
            item
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_visible_for_admin_on_hosts() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-admin");

    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty());

    for item in items {
        assert!(
            item.get("mac").is_some(),
            "Admin should see mac field on /hosts"
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_visible_for_admin_on_interfaces() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-admin");

    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();

    // If interfaces exist, Admin should see mac field
    for item in items {
        assert!(
            item.get("mac").is_some(),
            "Admin should see mac field on /interfaces"
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_visible_for_operator_on_devices() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-operator");

    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty());

    for item in items {
        assert!(item.get("mac").is_some(), "Operator should see mac field");
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_visible_for_operator_on_hosts() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-operator");

    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty());

    for item in items {
        assert!(
            item.get("mac").is_some(),
            "Operator should see mac field on /hosts"
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_visible_for_operator_on_interfaces() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-operator");

    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();

    // If interfaces exist, Operator should see mac field
    for item in items {
        assert!(
            item.get("mac").is_some(),
            "Operator should see mac field on /interfaces"
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_absent_for_viewer_on_devices() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-viewer");

    let resp = client
        .get(format!("{}/api/v1/devices", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty());

    // Viewer should NOT have mac field (skip_serializing_if = "Option::is_none")
    for item in items {
        assert!(
            item.get("mac").is_none() || item.get("mac") == Some(&Value::Null),
            "Viewer should NOT see mac field (should be absent or null): {:?}",
            item
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_absent_for_viewer_on_hosts() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-viewer");

    let resp = client
        .get(format!("{}/api/v1/hosts", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    assert!(!items.is_empty());

    for item in items {
        assert!(
            item.get("mac").is_none() || item.get("mac") == Some(&Value::Null),
            "Viewer should NOT see mac field on /hosts: {:?}",
            item
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_mac_absent_for_viewer_on_interfaces() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-viewer");

    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .headers(headers)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();

    // If interfaces exist, Viewer should NOT see mac field
    for item in items {
        assert!(
            item.get("mac").is_none() || item.get("mac") == Some(&Value::Null),
            "Viewer should NOT see mac field on /interfaces: {:?}",
            item
        );
    }

    handle.abort();
}

#[tokio::test]
async fn inventory_present_filter_interfaces() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-admin");

    // Test present=true filter
    let resp = client
        .get(format!("{}/api/v1/interfaces?present=true", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    for item in items {
        assert_eq!(
            item.get("present"),
            Some(&Value::Bool(true)),
            "present=true should only return present items"
        );
    }

    // Test present=false filter
    let resp = client
        .get(format!("{}/api/v1/interfaces?present=false", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    for item in items {
        assert_eq!(
            item.get("present"),
            Some(&Value::Bool(false)),
            "present=false should only return absent items"
        );
    }

    // Test invalid present value (abc) -> should return 400
    let resp = client
        .get(format!("{}/api/v1/interfaces?present=abc", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "present=abc should return 400");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Invalid present filter value: abc");

    // Test invalid present value (very long) -> should return 400 with truncated error message
    let long_value = "a".repeat(100);
    let resp = client
        .get(format!(
            "{}/api/v1/interfaces?present={}",
            base_url, long_value
        ))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "present=long should return 400");
    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap();
    assert!(error_msg.starts_with("Invalid present filter value: "));
    // Error message should be truncated to reasonable length (value part <= 32 chars)
    let prefix_len = "Invalid present filter value: ".len();
    let value_part = &error_msg[prefix_len..];
    assert!(
        value_part.len() <= 32,
        "Error message value part should be truncated to 32 chars, got: {}",
        value_part.len()
    );

    handle.abort();
}

#[tokio::test]
async fn inventory_present_filter_routes() {
    let (base_url, handle) = spawn_test_server_with_roles().await;
    let client = Client::new();
    let headers = make_auth_headers("test-token-admin");

    // Test present=true filter
    let resp = client
        .get(format!("{}/api/v1/routes?present=true", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    for item in items {
        assert_eq!(
            item.get("present"),
            Some(&Value::Bool(true)),
            "present=true should only return present items"
        );
    }

    // Test present=false filter
    let resp = client
        .get(format!("{}/api/v1/routes?present=false", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: Value = resp.json().await.unwrap();
    let items = body["items"].as_array().unwrap();
    for item in items {
        assert_eq!(
            item.get("present"),
            Some(&Value::Bool(false)),
            "present=false should only return absent items"
        );
    }

    // Test invalid present value (abc) -> should return 400
    let resp = client
        .get(format!("{}/api/v1/routes?present=abc", base_url))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "present=abc should return 400");
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Invalid present filter value: abc");

    // Test invalid present value (very long) -> should return 400 with truncated error message
    let long_value = "a".repeat(100);
    let resp = client
        .get(format!("{}/api/v1/routes?present={}", base_url, long_value))
        .headers(headers.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "present=long should return 400");
    let body: Value = resp.json().await.unwrap();
    let error_msg = body["error"].as_str().unwrap();
    assert!(error_msg.starts_with("Invalid present filter value: "));
    // Error message should be truncated to reasonable length (value part <= 32 chars)
    let prefix_len = "Invalid present filter value: ".len();
    let value_part = &error_msg[prefix_len..];
    assert!(
        value_part.len() <= 32,
        "Error message value part should be truncated to 32 chars, got: {}",
        value_part.len()
    );

    handle.abort();
}
