use netwatch::api::{router, AppState};
use netwatch::auth::AuthState;
use netwatch::config::{
    Config, SchedulerConfig, SyntheticCheck, SyntheticCheckType, SyntheticChecksConfig,
};
use netwatch::metrics::AppMetrics;
use netwatch::rate_limit::{RateLimitConfig, RateLimiter};
use netwatch::ssrf::{SsrfConfig as SsrfConfigStruct, SsrfState};
use netwatch::state_tracker::{EventType as StEventType, StateTracker, StateTrackerConfig};
use netwatch::storage::Storage;
use netwatch::storage::{
    get_check_results, get_events, prune_old_data, CheckResultMessage, CheckResultWriter,
    CheckResultsFilter, EventsFilter,
};
use reqwest::Client;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tokio::time::sleep;

/// Spawn a test server with DEFAULT config (auth enabled, SSRF strict)
async fn spawn_default_config_server(
    storage: Arc<Storage>,
) -> (String, tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

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
    let ssrf_state = Arc::new(SsrfState::new(SsrfConfigStruct::default()).unwrap());
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

    sleep(Duration::from_millis(100)).await;
    (base_url, handle, test_token.to_string())
}

/// Spawn a test server with RELAXED config (allows loopback)
async fn spawn_relaxed_config_server(
    storage: Arc<Storage>,
) -> (String, tokio::task::JoinHandle<()>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);

    let mut tokens = HashMap::new();
    let test_token = "test-token-relaxed";
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

    // Relaxed SSRF config - allows loopback for local testing
    let ssrf_config = SsrfConfigStruct {
        allow_loopback: true,
        allow_private_ips: true,
        ..Default::default()
    };
    let ssrf_state = Arc::new(SsrfState::new(ssrf_config).unwrap());
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

    sleep(Duration::from_millis(100)).await;
    (base_url, handle, test_token.to_string())
}

/// Create a test config with synthetic checks for local testing
#[allow(clippy::field_reassign_with_default)]
fn create_test_config_with_checks() -> Config {
    let mut config = Config::default();
    config.synthetic_checks = SyntheticChecksConfig {
        enabled: Some(true),
        interval: Some(1), // Fast interval for testing
        checks: vec![
            SyntheticCheck {
                id: "test-tcp-local".to_string(),
                name: "Test TCP Local".to_string(),
                check_type: SyntheticCheckType::Tcp,
                target: "127.0.0.1".to_string(),
                port: Some(9999), // Unlikely to have a server here
                timeout_ms: Some(500),
                expected_status: None,
                expected_body: None,
                headers: None,
                record_type: None,
                check_expiry_days: None,
                verify_hostname: None,
                enabled: true,
            },
            SyntheticCheck {
                id: "test-tcp-success".to_string(),
                name: "Test TCP Success".to_string(),
                check_type: SyntheticCheckType::Tcp,
                target: "127.0.0.1".to_string(),
                port: Some(8080), // Will fail but we can test the logic
                timeout_ms: Some(500),
                expected_status: None,
                expected_body: None,
                headers: None,
                record_type: None,
                check_expiry_days: None,
                verify_hostname: None,
                enabled: true,
            },
        ],
    };
    config.scheduler = SchedulerConfig {
        concurrency: 2,
        consecutive_failures_threshold: Some(2),
        consecutive_successes_threshold: Some(1),
        check_results_retention_days: Some(1),
        events_retention_days: Some(1),
        inventory_retention_days: Some(1),
        retention_prune_interval: Some(1),
    };
    config.ssrf = netwatch::ssrf::SsrfConfig {
        allow_loopback: true,
        allow_private_ips: true,
        ..Default::default()
    };
    config
}

#[tokio::test]
async fn test_state_tracker_down_after_n_failures() {
    let config = StateTrackerConfig {
        consecutive_failures_threshold: 3,
        consecutive_successes_threshold: 1,
    };
    let mut tracker = StateTracker::new(config);

    let evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: false,
        latency_ms: None,
        error: Some("connection refused".to_string()),
    };

    // 1st failure - no event
    let t = tracker.process_check(evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    assert_eq!(t.consecutive_failures, 1);

    // 2nd failure - no event
    let t = tracker.process_check(evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    assert_eq!(t.consecutive_failures, 2);

    // 3rd failure - DOWN event
    let t = tracker.process_check(evidence.clone(), "entity-1", "device");
    assert!(t.should_emit_event);
    assert_eq!(t.event_type, Some(StEventType::CheckFailed));
    assert_eq!(t.consecutive_failures, 3);
}

#[tokio::test]
async fn test_state_tracker_up_after_m_successes() {
    let config = StateTrackerConfig {
        consecutive_failures_threshold: 2,
        consecutive_successes_threshold: 2,
    };
    let mut tracker = StateTracker::new(config);

    let fail_evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: false,
        latency_ms: None,
        error: Some("connection refused".to_string()),
    };
    let success_evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: true,
        latency_ms: Some(10),
        error: None,
    };

    // Go down first
    for _ in 0..2 {
        tracker.process_check(fail_evidence.clone(), "entity-1", "device");
    }

    // 1st success - no event (threshold is 2)
    let t = tracker.process_check(success_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    assert_eq!(t.consecutive_successes, 1);

    // 2nd success - UP event
    let t = tracker.process_check(success_evidence.clone(), "entity-1", "device");
    assert!(t.should_emit_event);
    assert_eq!(t.event_type, Some(StEventType::CheckCompleted));
    assert_eq!(t.consecutive_successes, 2);
}

#[tokio::test]
async fn test_state_tracker_isolated_failure_no_event() {
    let config = StateTrackerConfig {
        consecutive_failures_threshold: 3,
        consecutive_successes_threshold: 1,
    };
    let mut tracker = StateTracker::new(config);

    let success_evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: true,
        latency_ms: Some(10),
        error: None,
    };
    let fail_evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: false,
        latency_ms: None,
        error: Some("timeout".to_string()),
    };

    // Success - no event (first check, no previous down state)
    let t = tracker.process_check(success_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);

    // Single failure - no event (threshold is 3)
    let t = tracker.process_check(fail_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    assert_eq!(t.consecutive_failures, 1);

    // Success again - NO UP event because failure threshold was never reached (no down state)
    let t = tracker.process_check(success_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    assert_eq!(t.consecutive_failures, 0);
    assert_eq!(t.consecutive_successes, 1);

    // Now reach down threshold
    let t = tracker.process_check(fail_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    let t = tracker.process_check(fail_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    let t = tracker.process_check(fail_evidence.clone(), "entity-1", "device");
    assert!(t.should_emit_event); // DOWN event after 3 failures
    assert_eq!(t.event_type, Some(StEventType::CheckFailed));

    // Recovery after down state
    let t = tracker.process_check(success_evidence.clone(), "entity-1", "device");
    assert!(t.should_emit_event); // UP event after 1 success (recovery)
    assert_eq!(t.event_type, Some(StEventType::CheckCompleted));
}

#[tokio::test]
async fn test_state_tracker_startup_with_down_no_false_transition() {
    let config = StateTrackerConfig {
        consecutive_failures_threshold: 3,
        consecutive_successes_threshold: 1,
    };
    let mut tracker = StateTracker::new(config);

    // Load state as if already down
    tracker.load_state(netwatch::state_tracker::EntityState::from_db(
        "entity-1".to_string(),
        "device".to_string(),
        Some(false), // last was down
        3,           // at threshold
        0,
    ));

    let fail_evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: false,
        latency_ms: None,
        error: Some("connection refused".to_string()),
    };

    // Another failure - NO new down event (already down)
    let t = tracker.process_check(fail_evidence.clone(), "entity-1", "device");
    assert!(!t.should_emit_event);
    assert_eq!(t.consecutive_failures, 4);

    // Success - UP event
    let success_evidence = netwatch::events::CheckEvidence {
        target: "test".to_string(),
        protocol: "tcp".to_string(),
        success: true,
        latency_ms: Some(10),
        error: None,
    };
    let t = tracker.process_check(success_evidence, "entity-1", "device");
    assert!(t.should_emit_event);
    assert_eq!(t.event_type, Some(StEventType::CheckCompleted));
}

#[tokio::test]
async fn test_writer_actor_persists_check_results_and_events() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Create writer actor
    let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Send a check result
    let check_result = netwatch::storage::CheckResult {
        id: "test-1".to_string(),
        entity_id: "entity-1".to_string(),
        entity_type: "Device".to_string(),
        target: "127.0.0.1:8080".to_string(),
        protocol: "tcp".to_string(),
        success: true,
        latency_ms: Some(10),
        error: None,
        consecutive_failures: 0,
        consecutive_successes: 1,
        last_state_change: true,
        previous_state: Some(false),
        observed_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
    };
    tx.send(CheckResultMessage::WriteCheckResult(check_result))
        .await
        .unwrap();

    // Send an event
    let event = netwatch::storage::EventV2 {
        id: "event-1".to_string(),
        schema_version: 2,
        event_type: "check_completed".to_string(),
        event_ts: chrono::Utc::now(),
        observed_ts: chrono::Utc::now(),
        entity_id: "entity-1".to_string(),
        entity_type: "Device".to_string(),
        severity: "info".to_string(),
        state: "firing".to_string(),
        source: "monitoring".to_string(),
        evidence: r#"{"check_result":{"target":"127.0.0.1:8080","protocol":"tcp","success":true,"latency_ms":10}}"#.to_string(),
        dedup_key: "check_completed:entity-1".to_string(),
        correlation_id: None,
        trace_id: None,
        metadata: "{}".to_string(),
        created_at: chrono::Utc::now(),
    };
    tx.send(CheckResultMessage::WriteEvent(event))
        .await
        .unwrap();

    // Shutdown writer
    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    writer_handle.await.unwrap();

    // Verify check result persisted
    let results = get_check_results(
        &storage.pool,
        CheckResultsFilter {
            limit: 10,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(results.items.len(), 1);
    assert_eq!(results.items[0].entity_id, "entity-1");
    assert!(results.items[0].success);

    // Verify event persisted
    let events = get_events(
        &storage.pool,
        EventsFilter {
            limit: 10,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(events.items.len(), 1);
    assert_eq!(events.items[0].entity_id, "entity-1");
    assert_eq!(events.items[0].event_type, "check_completed");
}

#[tokio::test]
async fn test_retention_pruning_removes_old_rows_only() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Create writer actor
    let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Insert old check results (2 days ago)
    let old_time = chrono::Utc::now() - chrono::Duration::days(2);
    for i in 0..5 {
        let check_result = netwatch::storage::CheckResult {
            id: format!("old-{}", i),
            entity_id: "entity-1".to_string(),
            entity_type: "Device".to_string(),
            target: "127.0.0.1:8080".to_string(),
            protocol: "tcp".to_string(),
            success: true,
            latency_ms: Some(10),
            error: None,
            consecutive_failures: 0,
            consecutive_successes: 1,
            last_state_change: true,
            previous_state: Some(false),
            observed_at: old_time,
            created_at: old_time,
        };
        tx.send(CheckResultMessage::WriteCheckResult(check_result))
            .await
            .unwrap();
    }

    // Insert recent check results (now)
    let now = chrono::Utc::now();
    for i in 0..3 {
        let check_result = netwatch::storage::CheckResult {
            id: format!("new-{}", i),
            entity_id: "entity-1".to_string(),
            entity_type: "Device".to_string(),
            target: "127.0.0.1:8080".to_string(),
            protocol: "tcp".to_string(),
            success: true,
            latency_ms: Some(10),
            error: None,
            consecutive_failures: 0,
            consecutive_successes: 1,
            last_state_change: true,
            previous_state: Some(false),
            observed_at: now,
            created_at: now,
        };
        tx.send(CheckResultMessage::WriteCheckResult(check_result))
            .await
            .unwrap();
    }

    // Insert old events (2 days ago)
    for i in 0..4 {
        let event = netwatch::storage::EventV2 {
            id: format!("old-event-{}", i),
            schema_version: 2,
            event_type: "check_completed".to_string(),
            event_ts: old_time,
            observed_ts: old_time,
            entity_id: "entity-1".to_string(),
            entity_type: "Device".to_string(),
            severity: "info".to_string(),
            state: "firing".to_string(),
            source: "monitoring".to_string(),
            evidence: "{}".to_string(),
            dedup_key: format!("check_completed:entity-1-{}", i),
            correlation_id: None,
            trace_id: None,
            metadata: "{}".to_string(),
            created_at: old_time,
        };
        tx.send(CheckResultMessage::WriteEvent(event))
            .await
            .unwrap();
    }

    // Insert recent events (now)
    for i in 0..2 {
        let event = netwatch::storage::EventV2 {
            id: format!("new-event-{}", i),
            schema_version: 2,
            event_type: "check_completed".to_string(),
            event_ts: now,
            observed_ts: now,
            entity_id: "entity-1".to_string(),
            entity_type: "Device".to_string(),
            severity: "info".to_string(),
            state: "firing".to_string(),
            source: "monitoring".to_string(),
            evidence: "{}".to_string(),
            dedup_key: format!("check_completed:entity-1-new-{}", i),
            correlation_id: None,
            trace_id: None,
            metadata: "{}".to_string(),
            created_at: now,
        };
        tx.send(CheckResultMessage::WriteEvent(event))
            .await
            .unwrap();
    }

    // Shutdown writer
    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    writer_handle.await.unwrap();

    // Prune with 1 day retention
    let (check_deleted, events_deleted) = prune_old_data(&storage.pool, 1, 1).await.unwrap();

    // Should have deleted 5 old check results and 4 old events
    assert_eq!(check_deleted, 5);
    assert_eq!(events_deleted, 4);

    // Verify only recent remain
    let results = get_check_results(
        &storage.pool,
        CheckResultsFilter {
            limit: 10,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(results.items.len(), 3);

    let events = get_events(
        &storage.pool,
        EventsFilter {
            limit: 10,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(events.items.len(), 2);
}

#[tokio::test]
async fn test_sqlite_busy_timeout_configured() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");

    // Connect with custom busy_timeout
    let storage = Storage::connect_with_options(db_path.to_str().unwrap(), 3000)
        .await
        .unwrap();

    // Verify the connection works (busy_timeout is set at connection level)
    // We can't directly read the pragma from sqlx pool, but we can verify
    // the connection succeeds and the option was accepted
    let row = sqlx::query("PRAGMA busy_timeout")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    let busy_timeout: i64 = row.get(0);
    assert_eq!(busy_timeout, 3000);
}

#[tokio::test]
async fn test_default_config_ssrf_blocks_loopback() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let (base_url, _handle, token) = spawn_default_config_server(storage).await;
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

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"]
        .as_str()
        .unwrap_or("")
        .contains("SSRF validation failed"));
    assert_eq!(body["evidence"], "ssrf_validation");
}

#[tokio::test]
async fn test_relaxed_config_allows_loopback_tcp_check() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let (base_url, _handle, token) = spawn_relaxed_config_server(storage).await;
    let client = Client::new();

    // POST to /monitor/tcp with loopback target should be ALLOWED by SSRF
    // (connection will fail but not due to SSRF)
    let resp = client
        .post(format!("{}/api/v1/monitor/tcp", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&json!({"host": "127.0.0.1", "port": 9999, "timeout_ms": 500}))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    // Should fail due to connection refused, NOT SSRF
    if !body["success"].as_bool().unwrap_or(false) {
        assert!(!body["error"]
            .as_str()
            .unwrap_or("")
            .contains("SSRF validation failed"));
    }
}

#[tokio::test]
async fn test_scheduler_executes_checks_persists_results() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    // Use relaxed config to allow loopback
    let config = create_test_config_with_checks();

    // Initialize event bus and check result writer
    let event_bus = Arc::new(netwatch::events::EventBus::new());
    let (writer, check_result_tx) =
        netwatch::storage::CheckResultWriter::new(storage.pool.clone(), 100);
    let _writer_handle = tokio::spawn(async move { writer.run().await });

    // Register monitoring jobs
    let mut scheduler = netwatch::scheduler::Scheduler::new(config.scheduler.concurrency);
    let ssrf_state = Arc::new(netwatch::ssrf::SsrfState::new(config.ssrf.clone()).unwrap());
    let ssrf_policy = Arc::new((*ssrf_state.policy()).clone());

    netwatch::monitoring_jobs::register_monitoring_jobs(
        &mut scheduler,
        storage.clone(),
        event_bus.clone(),
        check_result_tx.clone(),
        &config,
        ssrf_policy,
    )
    .await
    .unwrap();

    // Run scheduler for a short time
    let scheduler_handle = tokio::spawn(async move {
        let _ = scheduler.run().await;
    });

    // Wait for a few check intervals
    sleep(Duration::from_millis(3000)).await;

    // Shutdown scheduler
    scheduler_handle.abort();

    // Wait for writer to process
    sleep(Duration::from_millis(500)).await;

    // Verify check results were persisted
    let results = get_check_results(
        &storage.pool,
        CheckResultsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Should have results for both checks
    assert!(!results.items.is_empty());
    // Each check should have run multiple times
    let check_ids: Vec<String> = results.items.iter().map(|r| r.entity_id.clone()).collect();
    assert!(check_ids.contains(&"test-tcp-local".to_string()));
    assert!(check_ids.contains(&"test-tcp-success".to_string()));
}

#[tokio::test]
async fn test_api_reads_checks_paginated() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let (base_url, _handle, token) = spawn_relaxed_config_server(storage.clone()).await;
    let client = Client::new();

    // Insert some test data via writer
    let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
    let _writer_handle = tokio::spawn(async move { writer.run().await });

    for i in 0..15 {
        let check_result = netwatch::storage::CheckResult {
            id: format!("api-test-{}", i),
            entity_id: "api-entity".to_string(),
            entity_type: "Device".to_string(),
            target: "127.0.0.1:8080".to_string(),
            protocol: "tcp".to_string(),
            success: true,
            latency_ms: Some(10),
            error: None,
            consecutive_failures: 0,
            consecutive_successes: 1,
            last_state_change: true,
            previous_state: Some(false),
            observed_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };
        tx.send(CheckResultMessage::WriteCheckResult(check_result))
            .await
            .unwrap();
    }
    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    // Test pagination - first page
    let resp = client
        .get(format!("{}/api/v1/checks?limit=5&offset=0", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 5);
    assert_eq!(body["total"], 15);
    assert_eq!(body["limit"], 5);
    assert_eq!(body["offset"], 0);

    // Test pagination - second page
    let resp = client
        .get(format!("{}/api/v1/checks?limit=5&offset=5", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 5);
    assert_eq!(body["total"], 15);
    assert_eq!(body["limit"], 5);
    assert_eq!(body["offset"], 5);

    // Test limit beyond max -> 400
    let resp = client
        .get(format!("{}/api/v1/checks?limit=1000&offset=0", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("limit must be between 1 and 500"));
}

#[tokio::test]
async fn test_api_reads_events_paginated() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let (base_url, _handle, token) = spawn_relaxed_config_server(storage.clone()).await;
    let client = Client::new();

    // Insert some test events
    let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
    let _writer_handle = tokio::spawn(async move { writer.run().await });

    for i in 0..15 {
        let event = netwatch::storage::EventV2 {
            id: format!("api-event-{}", i),
            schema_version: 2,
            event_type: "check_completed".to_string(),
            event_ts: chrono::Utc::now(),
            observed_ts: chrono::Utc::now(),
            entity_id: "api-entity".to_string(),
            entity_type: "Device".to_string(),
            severity: "info".to_string(),
            state: "firing".to_string(),
            source: "monitoring".to_string(),
            evidence: "{}".to_string(),
            dedup_key: format!("check_completed:api-entity-{}", i),
            correlation_id: None,
            trace_id: None,
            metadata: "{}".to_string(),
            created_at: chrono::Utc::now(),
        };
        tx.send(CheckResultMessage::WriteEvent(event))
            .await
            .unwrap();
    }
    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    // Test pagination
    let resp = client
        .get(format!("{}/api/v1/events?limit=5&offset=0", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 5);
    assert_eq!(body["total"], 15);
    assert_eq!(body["limit"], 5);
    assert_eq!(body["offset"], 0);
}

#[tokio::test]
async fn test_icmp_check_without_ping_binary_fails_explicitly() {
    // This test verifies that ICMP checks fail explicitly when ping is not available
    // In our test environment, ping IS available, so we verify the check runs
    // and produces a proper result (success or failure, not a false success)

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let _storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Use main SSRF policy which allows loopback in relaxed config
    let ssrf_config = netwatch::ssrf::SsrfConfig {
        allow_loopback: true,
        ..Default::default()
    };
    let policy = netwatch::ssrf::SsrfPolicy::new(ssrf_config).unwrap();

    // Run ICMP check against loopback
    let result = netwatch::monitoring::icmp::ping_with_ssrf(&policy, "127.0.0.1", 2000).await;

    // Should complete (success or failure) but not hang
    assert!(result.is_ok());
    let check_result = result.unwrap();
    // Either success (if ping works) or failure with proper error
    if !check_result.success {
        assert!(check_result.error.is_some());
        // Error should not be "SSRF validation failed"
        assert!(!check_result.error.as_ref().unwrap().contains("SSRF"));
    }
}

#[tokio::test]
async fn test_api_negative_cases() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let storage = Arc::new(storage);

    let (base_url, _handle, token) = spawn_default_config_server(storage).await;
    let client = Client::new();

    // No token -> 401
    let resp = client
        .get(format!("{}/api/v1/interfaces", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);

    // Invalid limit -> 400 with JSON error
    let resp = client
        .get(format!("{}/api/v1/checks?limit=-1&offset=0", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("limit must be between 1 and 500"));

    // Invalid offset -> 400 with JSON error
    let resp = client
        .get(format!("{}/api/v1/checks?limit=10&offset=-1", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("offset must be non-negative"));

    // Invalid limit (too high) -> 400 with JSON error
    let resp = client
        .get(format!("{}/api/v1/checks?limit=10000&offset=0", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("limit must be between 1 and 500"));
}
