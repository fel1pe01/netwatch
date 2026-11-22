use chrono::Utc;
use netwatch::events::CheckEvidence;
use netwatch::state_tracker::{EventType as StEventType, StateTracker, StateTrackerConfig};
use netwatch::storage::{
    get_check_results, get_events, load_last_check_state, prune_old_data, CheckResultMessage,
    CheckResultWriter, CheckResultsFilter, EventsFilter, Storage,
};
use sqlx::{sqlite::SqlitePoolOptions, Row};
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::sleep;

/// Test that check_results and events_v2 survive process restart (writer actor shutdown + reconnect)
#[tokio::test]
async fn test_check_results_and_events_survive_restart() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.sqlite");

    // --- First "process" ---
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
    let writer_handle = tokio::spawn(async move { writer.run().await });

    // Write some check results
    for i in 0..10 {
        let cr = netwatch::storage::CheckResult {
            id: format!("cr-{}", i),
            entity_id: "entity-1".to_string(),
            entity_type: "Device".to_string(),
            target: "127.0.0.1:8080".to_string(),
            protocol: "tcp".to_string(),
            success: i % 2 == 0,
            latency_ms: Some(10 + i as u128),
            error: if i % 2 == 0 {
                None
            } else {
                Some("timeout".to_string())
            },
            consecutive_failures: if i % 2 == 0 { 0 } else { i as u32 },
            consecutive_successes: if i % 2 == 0 { i as u32 } else { 0 },
            last_state_change: i == 3 || i == 6,
            previous_state: if i == 0 { None } else { Some(i % 2 == 1) },
            observed_at: Utc::now(),
            created_at: Utc::now(),
        };
        tx.send(CheckResultMessage::WriteCheckResult(cr))
            .await
            .unwrap();
    }

    // Write some events
    for i in 0..5 {
        let ev = netwatch::storage::EventV2 {
            id: format!("ev-{}", i),
            schema_version: 2,
            event_type: if i % 2 == 0 {
                "check_completed"
            } else {
                "check_failed"
            }
            .to_string(),
            event_ts: Utc::now(),
            observed_ts: Utc::now(),
            entity_id: "entity-1".to_string(),
            entity_type: "Device".to_string(),
            severity: if i % 2 == 0 { "info" } else { "warning" }.to_string(),
            state: if i % 2 == 0 { "resolved" } else { "firing" }.to_string(),
            source: "monitoring".to_string(),
            evidence: r#"{"check_result":{"target":"127.0.0.1:8080"}}"#.to_string(),
            dedup_key: format!("check:entity-1:{}", i),
            correlation_id: None,
            trace_id: None,
            metadata: "{}".to_string(),
            created_at: Utc::now(),
        };
        tx.send(CheckResultMessage::WriteEvent(ev)).await.unwrap();
    }

    // Shutdown writer (simulates process shutdown)
    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    writer_handle.await.unwrap();

    // Wait a bit to ensure all writes are flushed
    sleep(Duration::from_millis(100)).await;

    // --- Second "process" (reconnect to same DB) ---
    let storage2 = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Verify check_results survived
    let results = get_check_results(
        &storage2.pool,
        CheckResultsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        results.items.len(),
        10,
        "check_results should survive restart"
    );

    // Verify data integrity
    for (i, cr) in results.items.iter().enumerate() {
        let expected_success = (9 - i) % 2 == 0; // reversed order due to DESC
        assert_eq!(
            cr.success, expected_success,
            "check_result {} success mismatch",
            i
        );
        assert_eq!(cr.entity_id, "entity-1");
        assert_eq!(cr.protocol, "tcp");
    }

    // Verify events survived
    let events = get_events(
        &storage2.pool,
        EventsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(events.items.len(), 5, "events_v2 should survive restart");

    // Verify event data integrity
    for (i, ev) in events.items.iter().enumerate() {
        let expected_type = if (4 - i) % 2 == 0 {
            "check_completed"
        } else {
            "check_failed"
        }; // reversed
        assert_eq!(ev.event_type, expected_type, "event {} type mismatch", i);
        assert_eq!(ev.entity_id, "entity-1");
    }
}

/// Test that migrations apply correctly on empty database
#[tokio::test]
async fn test_migrations_on_empty_database() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("empty_test.sqlite");

    // Connect to non-existent database (should create and run migrations)
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Verify all expected tables exist by querying them
    let tables = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .fetch_all(&storage.pool)
        .await
        .unwrap();

    let table_names: Vec<String> = tables.iter().map(|r| r.get::<String, _>("name")).collect();

    // Check core tables exist (routes is not a table - it's discovered from system)
    assert!(table_names.contains(&"devices".to_string()));
    assert!(table_names.contains(&"interfaces".to_string()));
    assert!(table_names.contains(&"observations".to_string()));
    assert!(table_names.contains(&"check_results".to_string()));
    assert!(table_names.contains(&"events_v2".to_string()));
    assert!(table_names.contains(&"incidents".to_string()));
    assert!(table_names.contains(&"topology_nodes".to_string()));
    assert!(table_names.contains(&"topology_edges".to_string()));
    assert!(table_names.contains(&"topology_snapshots".to_string()));

    // Verify local device was created
    let local_device = sqlx::query("SELECT * FROM devices WHERE id = 'local'")
        .fetch_optional(&storage.pool)
        .await
        .unwrap();
    assert!(
        local_device.is_some(),
        "local device should be created on connect"
    );
}

/// Test that migrations work correctly on existing database (upgrade scenario)
#[tokio::test]
async fn test_migrations_on_existing_database() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("upgrade_test.sqlite");

    // First, create a database with some data using an older schema approach
    // (simulate by creating and populating manually)
    {
        let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

        // Insert some test data into core tables
        sqlx::query(
            r#"
            INSERT INTO devices (id, hostname, ip, device_type, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("test-device-1")
        .bind("test-host")
        .bind("192.168.1.100")
        .bind("router")
        .bind("manual")
        .bind("high")
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&storage.pool)
        .await
        .unwrap();

        // Insert into check_results
        sqlx::query(
            r#"
            INSERT INTO check_results (
                id, entity_id, entity_type, target, protocol, success, latency_ms,
                error, consecutive_failures, consecutive_successes, last_state_change,
                previous_state, observed_at, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("old-cr-1")
        .bind("test-device-1")
        .bind("Device")
        .bind("192.168.1.100:8080")
        .bind("tcp")
        .bind(true)
        .bind(15)
        .bind(Option::<String>::None)
        .bind(0)
        .bind(5)
        .bind(false)
        .bind(Some(false))
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&storage.pool)
        .await
        .unwrap();

        // Insert into events_v2
        sqlx::query(
            r#"
            INSERT INTO events_v2 (
                id, schema_version, event_type, event_ts, observed_ts,
                entity_id, entity_type, severity, state, source,
                evidence, dedup_key, correlation_id, trace_id, metadata, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("old-ev-1")
        .bind(2)
        .bind("check_completed")
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind("test-device-1")
        .bind("Device")
        .bind("info")
        .bind("resolved")
        .bind("monitoring")
        .bind("{}")
        .bind("check:test-device-1:1")
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind("{}")
        .bind(Utc::now().to_rfc3339())
        .execute(&storage.pool)
        .await
        .unwrap();
    }

    // Now reconnect (simulates app upgrade with new migrations)
    let storage2 = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Verify old data still exists
    let old_device = sqlx::query("SELECT * FROM devices WHERE id = 'test-device-1'")
        .fetch_optional(&storage2.pool)
        .await
        .unwrap();
    assert!(
        old_device.is_some(),
        "existing device should survive migration"
    );

    let old_checks = get_check_results(
        &storage2.pool,
        CheckResultsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        old_checks.items.len(),
        1,
        "existing check_results should survive"
    );
    assert_eq!(old_checks.items[0].entity_id, "test-device-1");

    let old_events = get_events(
        &storage2.pool,
        EventsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        old_events.items.len(),
        1,
        "existing events_v2 should survive"
    );
    assert_eq!(old_events.items[0].entity_id, "test-device-1");

    // Verify new writes still work after upgrade
    let (writer, tx) = CheckResultWriter::new(storage2.pool.clone(), 100);
    let _writer_handle = tokio::spawn(async move { writer.run().await });

    let new_cr = netwatch::storage::CheckResult {
        id: "new-cr-1".to_string(),
        entity_id: "test-device-1".to_string(),
        entity_type: "Device".to_string(),
        target: "192.168.1.100:8080".to_string(),
        protocol: "tcp".to_string(),
        success: true,
        latency_ms: Some(20),
        error: None,
        consecutive_failures: 0,
        consecutive_successes: 6,
        last_state_change: false,
        previous_state: Some(true),
        observed_at: Utc::now(),
        created_at: Utc::now(),
    };
    tx.send(CheckResultMessage::WriteCheckResult(new_cr))
        .await
        .unwrap();
    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    let checks = get_check_results(
        &storage2.pool,
        CheckResultsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        checks.items.len(),
        2,
        "new writes should work after upgrade"
    );
}

/// Test that load_last_check_state works correctly after restart
#[tokio::test]
async fn test_load_last_state_after_restart() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("state_test.sqlite");

    // First process: write some check results with known state
    {
        let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
        let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
        let _writer_handle = tokio::spawn(async move { writer.run().await });

        // Write 3 failures then 1 success (should trigger down event at 3rd failure)
        for i in 0..4 {
            let success = i == 3;
            let cr = netwatch::storage::CheckResult {
                id: format!("state-cr-{}", i),
                entity_id: "entity-state-1".to_string(),
                entity_type: "Device".to_string(),
                target: "127.0.0.1:9999".to_string(),
                protocol: "tcp".to_string(),
                success,
                latency_ms: if success { Some(10) } else { None },
                error: if success {
                    None
                } else {
                    Some("timeout".to_string())
                },
                consecutive_failures: if success { 0 } else { i + 1 },
                consecutive_successes: if success { 1 } else { 0 },
                last_state_change: i == 2 || i == 3, // down at 3rd failure, up at success
                previous_state: if i == 0 { None } else { Some(i > 0) },
                observed_at: Utc::now(),
                created_at: Utc::now(),
            };
            tx.send(CheckResultMessage::WriteCheckResult(cr))
                .await
                .unwrap();
        }
        tx.send(CheckResultMessage::Shutdown).await.unwrap();
        sleep(Duration::from_millis(100)).await;
    }

    // Second process: load last state
    let storage2 = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    let loaded = load_last_check_state(&storage2.pool, "entity-state-1", "Device")
        .await
        .unwrap();

    assert!(loaded.is_some(), "should load last state");
    let (last_success, consecutive_failures, consecutive_successes) = loaded.unwrap();
    assert!(last_success, "last check was success");
    assert_eq!(consecutive_failures, 0, "failures reset on success");
    assert_eq!(consecutive_successes, 1, "one success");

    // Verify StateTracker can use this loaded state
    let config = StateTrackerConfig {
        consecutive_failures_threshold: 3,
        consecutive_successes_threshold: 1,
    };
    let mut tracker = StateTracker::new(config);

    // Load the state into tracker
    tracker.load_state(netwatch::state_tracker::EntityState::from_db(
        "entity-state-1".to_string(),
        "Device".to_string(),
        Some(last_success),
        consecutive_failures,
        consecutive_successes,
    ));

    // Next check (failure) should not emit event (only 1 failure)
    let fail_ev = CheckEvidence {
        target: "127.0.0.1:9999".to_string(),
        protocol: "tcp".to_string(),
        success: false,
        latency_ms: None,
        error: Some("timeout".to_string()),
    };
    let t = tracker.process_check(fail_ev.clone(), "entity-state-1", "Device");
    assert!(
        !t.should_emit_event,
        "1st failure after success should not emit event"
    );

    // Two more failures should emit down event
    let t = tracker.process_check(fail_ev.clone(), "entity-state-1", "Device");
    assert!(!t.should_emit_event, "2nd failure should not emit event");

    let t = tracker.process_check(fail_ev, "entity-state-1", "Device");
    assert!(t.should_emit_event, "3rd failure should emit down event");
    assert_eq!(t.event_type, Some(StEventType::CheckFailed));
}

/// Test that pruning works correctly and doesn't delete recent data
#[tokio::test]
async fn test_pruning_preserves_recent_data() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("prune_test.sqlite");

    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    let (writer, tx) = CheckResultWriter::new(storage.pool.clone(), 100);
    let _writer_handle = tokio::spawn(async move { writer.run().await });

    let now = Utc::now();
    let old_time = now - chrono::Duration::days(10);
    let recent_time = now - chrono::Duration::hours(1);

    // Insert old check results (should be pruned)
    for i in 0..5 {
        let cr = netwatch::storage::CheckResult {
            id: format!("old-cr-{}", i),
            entity_id: "entity-old".to_string(),
            entity_type: "Device".to_string(),
            target: "10.0.0.1:8080".to_string(),
            protocol: "tcp".to_string(),
            success: true,
            latency_ms: Some(10),
            error: None,
            consecutive_failures: 0,
            consecutive_successes: 1,
            last_state_change: false,
            previous_state: Some(true),
            observed_at: old_time,
            created_at: old_time,
        };
        tx.send(CheckResultMessage::WriteCheckResult(cr))
            .await
            .unwrap();
    }

    // Insert recent check results (should NOT be pruned)
    for i in 0..3 {
        let cr = netwatch::storage::CheckResult {
            id: format!("new-cr-{}", i),
            entity_id: "entity-new".to_string(),
            entity_type: "Device".to_string(),
            target: "10.0.0.2:8080".to_string(),
            protocol: "tcp".to_string(),
            success: true,
            latency_ms: Some(10),
            error: None,
            consecutive_failures: 0,
            consecutive_successes: 1,
            last_state_change: false,
            previous_state: Some(true),
            observed_at: recent_time,
            created_at: recent_time,
        };
        tx.send(CheckResultMessage::WriteCheckResult(cr))
            .await
            .unwrap();
    }

    // Insert old events (should be pruned)
    for i in 0..4 {
        let ev = netwatch::storage::EventV2 {
            id: format!("old-ev-{}", i),
            schema_version: 2,
            event_type: "check_completed".to_string(),
            event_ts: old_time,
            observed_ts: old_time,
            entity_id: "entity-old".to_string(),
            entity_type: "Device".to_string(),
            severity: "info".to_string(),
            state: "resolved".to_string(),
            source: "monitoring".to_string(),
            evidence: "{}".to_string(),
            dedup_key: format!("check:entity-old:{}", i),
            correlation_id: None,
            trace_id: None,
            metadata: "{}".to_string(),
            created_at: old_time,
        };
        tx.send(CheckResultMessage::WriteEvent(ev)).await.unwrap();
    }

    // Insert recent events (should NOT be pruned)
    for i in 0..2 {
        let ev = netwatch::storage::EventV2 {
            id: format!("new-ev-{}", i),
            schema_version: 2,
            event_type: "check_completed".to_string(),
            event_ts: recent_time,
            observed_ts: recent_time,
            entity_id: "entity-new".to_string(),
            entity_type: "Device".to_string(),
            severity: "info".to_string(),
            state: "resolved".to_string(),
            source: "monitoring".to_string(),
            evidence: "{}".to_string(),
            dedup_key: format!("check:entity-new:{}", i),
            correlation_id: None,
            trace_id: None,
            metadata: "{}".to_string(),
            created_at: recent_time,
        };
        tx.send(CheckResultMessage::WriteEvent(ev)).await.unwrap();
    }

    tx.send(CheckResultMessage::Shutdown).await.unwrap();
    sleep(Duration::from_millis(100)).await;

    // Prune with 1 day retention
    let (check_deleted, events_deleted) = prune_old_data(&storage.pool, 1, 1).await.unwrap();

    assert_eq!(check_deleted, 5, "should delete 5 old check_results");
    assert_eq!(events_deleted, 4, "should delete 4 old events");

    // Verify recent data survives
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
    assert_eq!(
        results.items.len(),
        3,
        "recent check_results should survive"
    );
    assert!(results.items.iter().all(|r| r.entity_id == "entity-new"));

    let events = get_events(
        &storage.pool,
        EventsFilter {
            limit: 100,
            offset: 0,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(events.items.len(), 2, "recent events_v2 should survive");
    assert!(events.items.iter().all(|e| e.entity_id == "entity-new"));
}

/// Test that migration deduplicates devices with duplicate IPs before creating unique indexes
/// Simulates an "old" database with devices having duplicate IPs and children hanging from both.
/// After migration: DB starts, PRAGMA foreign_key_check empty, unique indexes present, child data preserved.
#[tokio::test]
async fn test_migration_deduplicates_duplicate_ips() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("dedup_test.sqlite");

    // Step 1: Create a database with ONLY the initial migration applied
    // We do this by creating a custom migrator with just the initial migration file
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(db_path.to_str().unwrap())
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_millis(5000));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        // Create a migrator that only includes the initial migration
        let temp_migrations_dir = tempdir().unwrap();
        let initial_migration_src = std::path::Path::new("./migrations/20231006000000_initial.sql");
        let initial_migration_dst = temp_migrations_dir
            .path()
            .join("20231006000000_initial.sql");
        std::fs::copy(initial_migration_src, &initial_migration_dst).unwrap();

        let initial_migrator = sqlx::migrate::Migrator::new(temp_migrations_dir.path())
            .await
            .unwrap();
        initial_migrator.run(&pool).await.unwrap();

        pool.close().await;
    }

    // Step 2: Insert duplicate data into the database (after initial migration, before dedup migration)
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(db_path.to_str().unwrap())
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_millis(5000));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        // Insert duplicate devices with children (using the schema from initial migration)
        let now = Utc::now().to_rfc3339();
        let now_minus_1 = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let now_minus_2 = (Utc::now() - chrono::Duration::hours(2)).to_rfc3339();

        // Device 1 (keeper - most recent updated_at)
        sqlx::query(
            r#"
            INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("device-keeper")
        .bind("router-1")
        .bind("192.168.1.1")
        .bind("aa:bb:cc:dd:ee:ff")
        .bind("router")
        .bind("discovery")
        .bind("high")
        .bind(&now)
        .bind(&now_minus_2)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        // Device 2 (duplicate - older updated_at)
        sqlx::query(
            r#"
            INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("device-duplicate")
        .bind("router-1-dup")
        .bind("192.168.1.1")
        .bind("aa:bb:cc:dd:ee:00")
        .bind("router")
        .bind("discovery")
        .bind("high")
        .bind(&now_minus_1)
        .bind(&now_minus_2)
        .bind(&now_minus_1)
        .execute(&pool)
        .await
        .unwrap();

        // Insert interfaces for BOTH devices (will have collision on device_id+name after dedup)
        sqlx::query(
            r#"
            INSERT INTO interfaces (id, device_id, name, mac, mtu, state, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("iface-keeper-eth0")
        .bind("device-keeper")
        .bind("eth0")
        .bind("aa:bb:cc:dd:ee:ff")
        .bind(1500)
        .bind("up")
        .bind("discovery")
        .bind("high")
        .bind(&now)
        .bind(&now_minus_2)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO interfaces (id, device_id, name, mac, mtu, state, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("iface-dup-eth0")
        .bind("device-duplicate")
        .bind("eth0")  // SAME name - will collide after dedup
        .bind("aa:bb:cc:dd:ee:00")
        .bind(1500)
        .bind("up")
        .bind("discovery")
        .bind("high")
        .bind(&now_minus_1)
        .bind(&now_minus_2)
        .bind(&now_minus_1)
        .execute(&pool)
        .await
        .unwrap();

        // Insert another interface for duplicate device (unique name)
        sqlx::query(
            r#"
            INSERT INTO interfaces (id, device_id, name, mac, mtu, state, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("iface-dup-eth1")
        .bind("device-duplicate")
        .bind("eth1")
        .bind("aa:bb:cc:dd:ee:01")
        .bind(1500)
        .bind("up")
        .bind("discovery")
        .bind("high")
        .bind(&now_minus_1)
        .bind(&now_minus_2)
        .bind(&now_minus_1)
        .execute(&pool)
        .await
        .unwrap();

        // Insert services for BOTH devices (will have collision on device_id+port+protocol after dedup)
        sqlx::query(
            r#"
            INSERT INTO services (id, device_id, port, protocol, name, status, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("svc-keeper-80")
        .bind("device-keeper")
        .bind(80)
        .bind("tcp")
        .bind("http")
        .bind("open")
        .bind("discovery")
        .bind("high")
        .bind(&now)
        .bind(&now_minus_2)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO services (id, device_id, port, protocol, name, status, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("svc-dup-80")
        .bind("device-duplicate")
        .bind(80)  // SAME port+protocol - will collide after dedup
        .bind("tcp")
        .bind("http")
        .bind("open")
        .bind("discovery")
        .bind("high")
        .bind(&now_minus_1)
        .bind(&now_minus_2)
        .bind(&now_minus_1)
        .execute(&pool)
        .await
        .unwrap();

        // Insert another service for duplicate device (unique port)
        sqlx::query(
            r#"
            INSERT INTO services (id, device_id, port, protocol, name, status, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("svc-dup-443")
        .bind("device-duplicate")
        .bind(443)
        .bind("tcp")
        .bind("https")
        .bind("open")
        .bind("discovery")
        .bind("high")
        .bind(&now_minus_1)
        .bind(&now_minus_2)
        .bind(&now_minus_1)
        .execute(&pool)
        .await
        .unwrap();

        // Insert events for both devices
        sqlx::query(
            r#"
            INSERT INTO events (id, device_id, event_type, severity, timestamp, message, metadata, evidence, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("evt-keeper")
        .bind("device-keeper")
        .bind("check_completed")
        .bind("info")
        .bind(&now)
        .bind("check ok")
        .bind("{}")
        .bind("{}")
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO events (id, device_id, event_type, severity, timestamp, message, metadata, evidence, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("evt-dup")
        .bind("device-duplicate")
        .bind("check_completed")
        .bind("info")
        .bind(&now_minus_1)
        .bind("check ok")
        .bind("{}")
        .bind("{}")
        .bind(&now_minus_1)
        .bind(&now_minus_1)
        .execute(&pool)
        .await
        .unwrap();

        // Note: topology_nodes, topology_edges, observations, check_results, events_v2, incidents
        // tables don't exist yet in the initial migration schema.
        // They will be created by subsequent migrations.
        // We'll insert data for them AFTER those migrations run, but we need to test that
        // the dedup migration correctly handles data in those tables too.
        // For now, let's test the core tables that exist in initial migration.

        pool.close().await;
    }

    // Step 3: Now run the FULL migration suite (which includes our merged dedup+indexes migration)
    // This should deduplicate the devices and create unique indexes
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();

    // Verify the database started successfully (no unique constraint violation)
    // Check that only ONE device with IP 192.168.1.1 exists (the keeper)
    let device_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE ip = '192.168.1.1'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(
        device_count, 1,
        "should have exactly one device per IP after dedup"
    );

    // Verify the keeper device is the one with most recent updated_at
    let keeper = sqlx::query("SELECT id, hostname FROM devices WHERE ip = '192.168.1.1'")
        .fetch_one(&storage.pool)
        .await
        .unwrap();
    assert_eq!(keeper.get::<String, _>("id"), "device-keeper");
    assert_eq!(keeper.get::<String, _>("hostname"), "router-1");

    // Verify unique indexes exist
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='index' AND sql LIKE '%UNIQUE%' ORDER BY name",
    )
    .fetch_all(&storage.pool)
    .await
    .unwrap();
    assert!(
        indexes.contains(&"idx_devices_ip".to_string()),
        "idx_devices_ip unique index should exist"
    );
    assert!(
        indexes.contains(&"idx_interfaces_device_name".to_string()),
        "idx_interfaces_device_name unique index should exist"
    );
    assert!(
        indexes.contains(&"idx_services_device_port_proto".to_string()),
        "idx_services_device_port_proto unique index should exist"
    );
    assert!(
        indexes.contains(&"idx_routes_dest_iface_gw_metric".to_string()),
        "idx_routes_dest_iface_gw_metric unique index should exist"
    );

    // Verify PRAGMA foreign_key_check is empty (no FK violations)
    let fk_check: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&storage.pool)
        .await
        .unwrap();
    assert!(
        fk_check.is_empty(),
        "foreign_key_check should be empty, got {} rows",
        fk_check.len()
    );

    // Verify child data was preserved and reassigned to keeper
    // Interfaces: should have 2 (keeper's eth0 + duplicate's eth1, but duplicate's eth0 collides and should be deduped)
    let iface_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM interfaces WHERE device_id = 'device-keeper'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(
        iface_count, 2,
        "should have 2 interfaces for keeper (eth0 from keeper, eth1 from duplicate)"
    );

    let iface_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM interfaces WHERE device_id = 'device-keeper' ORDER BY name",
    )
    .fetch_all(&storage.pool)
    .await
    .unwrap();
    assert!(
        iface_names.contains(&"eth0".to_string()),
        "should have eth0"
    );
    assert!(
        iface_names.contains(&"eth1".to_string()),
        "should have eth1"
    );

    // Services: should have 2 (keeper's 80/tcp + duplicate's 443/tcp, but duplicate's 80/tcp collides and should be deduped)
    let svc_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM services WHERE device_id = 'device-keeper'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(
        svc_count, 2,
        "should have 2 services for keeper (80/tcp from keeper, 443/tcp from duplicate)"
    );

    let svc_ports: Vec<i64> = sqlx::query_scalar(
        "SELECT port FROM services WHERE device_id = 'device-keeper' ORDER BY port",
    )
    .fetch_all(&storage.pool)
    .await
    .unwrap();
    assert_eq!(svc_ports, vec![80, 443], "should have ports 80 and 443");

    // Verify events were reassigned
    let evt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE device_id = 'device-keeper'")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
    assert_eq!(
        evt_count, 2,
        "should have 2 events for keeper (from both original devices)"
    );

    // Verify routes table exists
    let routes_table: Option<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name='routes'")
            .fetch_optional(&storage.pool)
            .await
            .unwrap();
    assert!(routes_table.is_some(), "routes table should exist");

    // Verify last_seen columns exist
    let has_last_seen_devices: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('devices') WHERE name='last_seen'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(
        has_last_seen_devices, 1,
        "devices.last_seen column should exist"
    );

    let has_last_seen_interfaces: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('interfaces') WHERE name='last_seen'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(
        has_last_seen_interfaces, 1,
        "interfaces.last_seen column should exist"
    );

    let has_last_seen_routes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('routes') WHERE name='last_seen'",
    )
    .fetch_one(&storage.pool)
    .await
    .unwrap();
    assert_eq!(
        has_last_seen_routes, 1,
        "routes.last_seen column should exist"
    );
}
