use anyhow::Result;
use chrono::{Duration, Utc};
use netwatch::events::{EntityType, Event, EventBus, EventEvidence, EventType, Severity};
use netwatch::incidents::{
    confirm_root_cause, Incident, IncidentEngine, IncidentError, IncidentSeverity, IncidentState,
    IncidentStore,
};
use netwatch::storage::Storage;
use std::sync::Arc;
use tempfile::tempdir;

async fn setup_test_storage(dir: &tempfile::TempDir) -> Arc<Storage> {
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    Arc::new(storage)
}

fn create_test_event(entity_id: &str, event_type: EventType) -> Event {
    let severity = match event_type {
        EventType::CheckFailed => Severity::Warning,
        EventType::ThresholdBreached => Severity::Critical,
        EventType::AnomalyDetected => Severity::Warning,
        EventType::CheckCompleted => Severity::Info,
        EventType::ThresholdRecovered => Severity::Info,
        _ => Severity::Info,
    };

    let event_type_str = event_type.as_str();
    let mut event = Event::new(
        event_type,
        entity_id.to_string(),
        EntityType::Device,
        severity,
        "test".to_string(),
        EventEvidence::default(),
    );
    event.dedup_key = format!("{}:{}", event_type_str, entity_id);
    event
}

#[tokio::test]
async fn test_incident_state_machine() -> Result<()> {
    let mut incident = Incident::new(
        "test-dedup".to_string(),
        "test-device".to_string(),
        "device".to_string(),
        IncidentSeverity::Warning,
        "Test Incident".to_string(),
        Some("Test description".to_string()),
        vec![],
        Default::default(),
        None,
    );

    // Initial state
    assert_eq!(incident.state, IncidentState::Open);
    assert!(incident.transitions.is_empty());

    // Open -> Acknowledged
    incident.acknowledge("operator1".to_string(), Some("Investigating".to_string()))?;
    assert_eq!(incident.state, IncidentState::Acknowledged);
    assert_eq!(incident.acknowledged_by, Some("operator1".to_string()));
    assert_eq!(incident.transitions.len(), 1);
    assert_eq!(incident.transitions[0].from, IncidentState::Open);
    assert_eq!(incident.transitions[0].to, IncidentState::Acknowledged);
    assert_eq!(incident.transitions[0].actor, Some("operator1".to_string()));

    // Acknowledged -> Resolved
    incident.resolve("operator1".to_string(), Some("Fixed".to_string()))?;
    assert_eq!(incident.state, IncidentState::Resolved);
    assert_eq!(incident.resolved_by, Some("operator1".to_string()));
    assert_eq!(incident.transitions.len(), 2);

    // Resolved -> Closed
    incident.close("operator1".to_string(), Some("Verified".to_string()))?;
    assert_eq!(incident.state, IncidentState::Closed);
    assert_eq!(incident.transitions.len(), 3);

    // Invalid transition: Closed -> Open (not allowed)
    let result = incident.reopen("operator1".to_string(), Some("Reopened".to_string()));
    assert!(result.is_err());
    match result.unwrap_err() {
        IncidentError::InvalidTransition { from, to } => {
            assert_eq!(from, IncidentState::Closed);
            assert_eq!(to, IncidentState::Open);
        }
        _ => panic!("Expected InvalidTransition error"),
    }

    Ok(())
}

#[tokio::test]
async fn test_incident_persistence() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;

    // Debug: check the incidents table schema
    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('incidents')")
        .fetch_one(&storage.pool)
        .await?;
    eprintln!("Incidents table column count: {}", col_count);
    assert_eq!(col_count, 25, "Incidents table should have 25 columns");

    // Test raw INSERT with all 25 columns
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO incidents (\
            id, dedup_key, correlation_id, entity_id, entity_type, severity, state,\
            title, description, evidence, labels, created_at, updated_at,\
            acknowledged_at, acknowledged_by, resolved_at, resolved_by, closed_at,\
            transitions, cooldown_until, silence_until,\
            root_cause_hypothesis_id, root_cause_confirmed, root_cause_confirmed_at, root_cause_confirmed_by\
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("test-id")
    .bind("test-dedup")
    .bind("test-corr")
    .bind("test-entity")
    .bind("device")
    .bind("warning")
    .bind("open")
    .bind("Test Title")
    .bind("Test Description")
    .bind("[]")
    .bind("{}")
    .bind(&now)
    .bind(&now)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind("[]")
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(false)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .execute(&storage.pool)
    .await?;
    eprintln!("Raw INSERT with 25 columns succeeded");

    let store = IncidentStore::new(storage.pool.clone());

    let incident = Incident::new(
        "test-dedup-1".to_string(),
        "test-device".to_string(),
        "device".to_string(),
        IncidentSeverity::Critical,
        "Critical Incident".to_string(),
        Some("Description".to_string()),
        vec![serde_json::json!({"key": "value"})],
        Default::default(),
        Some("correlation-1".to_string()),
    );

    let created = store.create(&incident).await?;
    assert_eq!(created.id, incident.id);
    assert_eq!(created.dedup_key, incident.dedup_key);
    assert_eq!(created.correlation_id, incident.correlation_id);

    // Find by dedup key
    let found = store.find_by_dedup_key("test-dedup-1").await?.unwrap();
    assert_eq!(found.id, incident.id);

    // Find by ID
    let found = store.find_by_id(&incident.id).await?.unwrap();
    assert_eq!(found.dedup_key, "test-dedup-1");

    // Update incident
    let mut updated = found.clone();
    updated.acknowledge("operator1".to_string(), None)?;
    store.update(&updated).await?;

    let found = store.find_by_id(&incident.id).await?.unwrap();
    assert_eq!(found.state, IncidentState::Acknowledged);
    assert_eq!(found.acknowledged_by, Some("operator1".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_incident_deduplication() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    // Emit CheckFailed event
    let event = create_test_event("device-1", EventType::CheckFailed);
    event_bus.emit(event.clone()).await.unwrap();

    // Process event
    engine.process_event(event).await?;

    // Verify incident created
    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].entity_id, "device-1");
    assert_eq!(incidents[0].state, IncidentState::Open);

    // Emit same event again (within cooldown)
    let event2 = create_test_event("device-1", EventType::CheckFailed);
    event_bus.emit(event2.clone()).await.unwrap();
    engine.process_event(event2).await?;

    // Should NOT create a new incident (dedup)
    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_incident_auto_resolve() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    // Emit CheckFailed
    let event = create_test_event("device-1", EventType::CheckFailed);
    event_bus.emit(event).await.unwrap();
    engine
        .process_event(create_test_event("device-1", EventType::CheckFailed))
        .await?;

    // Verify incident is open
    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents[0].state, IncidentState::Open);

    // Emit CheckCompleted (recovery) with same dedup_key as firing event
    let mut recovery_event = create_test_event("device-1", EventType::CheckCompleted);
    recovery_event.dedup_key = "check_failed:device-1".to_string();
    event_bus.emit(recovery_event.clone()).await.unwrap();
    engine.process_event(recovery_event).await?;

    // Verify incident is resolved
    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents[0].state, IncidentState::Resolved);
    assert!(incidents[0].resolved_at.is_some());
    assert_eq!(incidents[0].resolved_by, Some("auto".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_incident_cooldown() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300); // 5 min cooldown

    // First event
    let event = create_test_event("device-1", EventType::CheckFailed);
    event_bus.emit(event.clone()).await.unwrap();
    engine.process_event(event).await?;

    // Verify cooldown is set
    let incidents = engine.store.list_open().await?;
    assert!(incidents[0].cooldown_until.is_some());
    assert!(incidents[0].is_in_cooldown());

    Ok(())
}

#[tokio::test]
async fn test_incident_silence() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    // Create incident
    let event = create_test_event("device-1", EventType::CheckFailed);
    event_bus.emit(event.clone()).await.unwrap();
    engine.process_event(event).await?;

    // Silence the incident
    let incidents = engine.store.list_open().await?;
    let mut incident = incidents[0].clone();
    incident.set_silence(600); // 10 minutes
    engine.store.update(&incident).await?;

    // Emit another failure event for same device
    let event2 = create_test_event("device-1", EventType::CheckFailed);
    event_bus.emit(event2.clone()).await.unwrap();
    engine.process_event(event2).await?;

    // Should not create new incident (silenced)
    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 1);
    assert!(incidents[0].is_silenced());

    Ok(())
}

#[tokio::test]
async fn test_multiple_incidents_different_entities() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    // Create incidents for different devices
    for i in 1..=3 {
        let event = create_test_event(&format!("device-{}", i), EventType::CheckFailed);
        event_bus.emit(event.clone()).await.unwrap();
        engine.process_event(event).await?;
    }

    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 3);

    let entity_ids: Vec<String> = incidents.iter().map(|i| i.entity_id.clone()).collect();
    assert!(entity_ids.contains(&"device-1".to_string()));
    assert!(entity_ids.contains(&"device-2".to_string()));
    assert!(entity_ids.contains(&"device-3".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_anomaly_detected_creates_incident() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    // Emit AnomalyDetected event
    let mut event = Event::new(
        EventType::AnomalyDetected,
        "device-1".to_string(),
        EntityType::Device,
        Severity::Warning,
        "anomaly_detector".to_string(),
        EventEvidence {
            anomaly: Some(netwatch::events::AnomalyEvidence {
                entity_id: "device-1".to_string(),
                entity_type: "device".to_string(),
                metric: "icmp_latency_ms".to_string(),
                protocol: "icmp".to_string(),
                anomaly_type: "spike".to_string(),
                severity: "warning".to_string(),
                current_value: 100.0,
                baseline_mean: 50.0,
                baseline_stddev: 10.0,
                z_score: 5.0,
                sample_count: 20,
                window_start: (Utc::now() - Duration::hours(1)).to_rfc3339(),
                window_end: Utc::now().to_rfc3339(),
                rule_id: "spike_icmp_latency_ms_icmp".to_string(),
                confidence: 0.9,
                description: "Spike detected".to_string(),
            }),
            check_result: None,
            threshold: None,
            topology_diff: None,
            collector_error: None,
            raw: None,
        },
    );
    event.dedup_key = "anomaly_detected:device-1".to_string();

    event_bus.emit(event.clone()).await.unwrap();
    engine.process_event(event).await?;

    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 1);
    assert!(incidents[0].title.contains("Anomaly detected"));
    assert!(incidents[0].title.contains("spike"));
    Ok(())
}

#[tokio::test]
async fn test_root_cause_confirmation() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let store = IncidentStore::new(storage.pool.clone());

    let incident = Incident::new(
        "test-dedup-rc".to_string(),
        "test-device".to_string(),
        "device".to_string(),
        IncidentSeverity::Warning,
        "Test Incident".to_string(),
        None,
        vec![],
        Default::default(),
        None,
    );
    store.create(&incident).await?;

    // Confirm root cause hypothesis
    let hypothesis_id = "hyp-123";
    confirm_root_cause(
        &store,
        &incident.id,
        hypothesis_id,
        true,
        "operator1".to_string(),
    )
    .await?;

    let updated = store.find_by_id(&incident.id).await?.unwrap();
    assert_eq!(
        updated.root_cause_hypothesis_id,
        Some(hypothesis_id.to_string())
    );
    assert!(updated.root_cause_confirmed);
    assert_eq!(
        updated.root_cause_confirmed_by,
        Some("operator1".to_string())
    );
    assert!(updated.root_cause_confirmed_at.is_some());

    Ok(())
}

#[tokio::test]
async fn test_incident_transition_history() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let store = IncidentStore::new(storage.pool.clone());

    let mut incident = Incident::new(
        "test-dedup-history".to_string(),
        "test-device".to_string(),
        "device".to_string(),
        IncidentSeverity::Warning,
        "Test Incident".to_string(),
        None,
        vec![],
        Default::default(),
        None,
    );
    store.create(&incident).await?;

    // Perform multiple transitions
    incident.acknowledge("operator1".to_string(), Some("Ack".to_string()))?;
    store.update(&incident).await?;

    incident.resolve("operator2".to_string(), Some("Resolved".to_string()))?;
    store.update(&incident).await?;

    incident.close("operator3".to_string(), Some("Closed".to_string()))?;
    store.update(&incident).await?;

    let found = store.find_by_id(&incident.id).await?.unwrap();
    assert_eq!(found.transitions.len(), 3);

    // Verify transition order and actors
    assert_eq!(found.transitions[0].from, IncidentState::Open);
    assert_eq!(found.transitions[0].to, IncidentState::Acknowledged);
    assert_eq!(found.transitions[0].actor, Some("operator1".to_string()));

    assert_eq!(found.transitions[1].from, IncidentState::Acknowledged);
    assert_eq!(found.transitions[1].to, IncidentState::Resolved);
    assert_eq!(found.transitions[1].actor, Some("operator2".to_string()));

    assert_eq!(found.transitions[2].from, IncidentState::Resolved);
    assert_eq!(found.transitions[2].to, IncidentState::Closed);
    assert_eq!(found.transitions[2].actor, Some("operator3".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_incident_correlation_id() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    // Emit events with same correlation_id
    let mut event1 = create_test_event("device-1", EventType::CheckFailed);
    event1.correlation_id = Some("corr-123".to_string());
    event_bus.emit(event1.clone()).await.unwrap();
    engine.process_event(event1).await?;

    let mut event2 = create_test_event("device-2", EventType::CheckFailed);
    event2.correlation_id = Some("corr-123".to_string());
    event_bus.emit(event2.clone()).await.unwrap();
    engine.process_event(event2).await?;

    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 2);

    for incident in incidents {
        assert_eq!(incident.correlation_id, Some("corr-123".to_string()));
    }

    Ok(())
}

#[tokio::test]
async fn test_threshold_breached_creates_incident() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let event_bus = Arc::new(EventBus::new());
    let receiver = event_bus.subscribe();
    let store = IncidentStore::new(storage.pool.clone());
    let mut engine = IncidentEngine::new(store, receiver, 300);

    let mut event = Event::new(
        EventType::ThresholdBreached,
        "device-1".to_string(),
        EntityType::Device,
        Severity::Critical,
        "monitoring".to_string(),
        EventEvidence {
            threshold: Some(netwatch::events::ThresholdEvidence {
                metric: "icmp_latency_ms".to_string(),
                value: 500.0,
                threshold: 200.0,
                operator: ">".to_string(),
            }),
            check_result: None,
            topology_diff: None,
            collector_error: None,
            anomaly: None,
            raw: None,
        },
    );
    event.dedup_key = "threshold_breached:device-1".to_string();

    event_bus.emit(event.clone()).await.unwrap();
    engine.process_event(event).await?;

    let incidents = engine.store.list_open().await?;
    assert_eq!(incidents.len(), 1);
    assert_eq!(incidents[0].severity, IncidentSeverity::Critical);
    assert!(incidents[0].title.contains("Threshold breached"));

    Ok(())
}
