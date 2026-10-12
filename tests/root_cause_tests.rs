use anyhow::Result;
use chrono::{Duration, Utc};
use netwatch::root_cause::RootCauseAnalyzer;
use netwatch::storage::{Observation, Storage};
use netwatch::topology::{
    NodeState, Provenance, TopologyEdge, TopologyEdgeType, TopologyNode, TopologyNodeType,
};
use netwatch::topology_store::TopologyStore;
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_test_storage(dir: &tempfile::TempDir) -> Arc<Storage> {
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    Arc::new(storage)
}

async fn ensure_device_exists(storage: &Storage, device_id: &str) {
    let row = sqlx::query("SELECT id FROM devices WHERE id = ?")
        .bind(device_id)
        .fetch_optional(&storage.pool)
        .await
        .unwrap();

    if row.is_none() {
        sqlx::query(
            r#"INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(device_id)
        .bind(device_id)
        .bind("127.0.0.1")
        .bind("00:00:00:00:00:00")
        .bind("test")
        .bind("test")
        .bind("high")
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&storage.pool)
        .await
        .unwrap();
    }
}

async fn insert_failing_observations(
    storage: &Storage,
    device_id: &str,
    protocol: &str,
    count: usize,
    base_time: chrono::DateTime<Utc>,
) {
    ensure_device_exists(storage, device_id).await;
    for i in 0..count {
        let obs = Observation {
            id: Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            metric: format!("{}_latency_ms", protocol),
            value: 0.0, // 0 indicates failure
            source: "test".to_string(),
            confidence: "low".to_string(), // low confidence = failure
            observed_at: base_time + Duration::seconds(i as i64 * 60),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        storage.insert_observation(&obs).await.unwrap();
    }
}

async fn setup_topology_with_gateway(topology_store: &TopologyStore) -> Result<()> {
    // Create gateway node
    let gateway = TopologyNode {
        id: "gateway-1".to_string(),
        node_type: TopologyNodeType::Gateway,
        label: "Gateway 1".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&gateway).await?;

    // Create device nodes
    for i in 1..=5 {
        let device = TopologyNode {
            id: format!("device-{}", i),
            node_type: TopologyNodeType::Device,
            label: format!("Device {}", i),
            properties: Default::default(),
            state: NodeState::Online,
            observed_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        topology_store.upsert_node(&device).await?;

        // Add GatewayOf edge from device to gateway
        let edge = TopologyEdge {
            id: format!("edge-{}-gateway", i),
            source: format!("device-{}", i),
            target: "gateway-1".to_string(),
            edge_type: TopologyEdgeType::GatewayOf,
            provenance: Provenance::RoutingTable,
            confidence: 0.9,
            observed_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        topology_store.upsert_edge(&edge).await?;
    }

    Ok(())
}

async fn setup_topology_isolated_device(topology_store: &TopologyStore) -> Result<()> {
    let device = TopologyNode {
        id: "isolated-device".to_string(),
        node_type: TopologyNodeType::Device,
        label: "Isolated Device".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&device).await?;
    Ok(())
}

async fn setup_topology_shared_dependency(topology_store: &TopologyStore) -> Result<()> {
    // Shared gateway
    let gateway = TopologyNode {
        id: "shared-gateway".to_string(),
        node_type: TopologyNodeType::Gateway,
        label: "Shared Gateway".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&gateway).await?;

    // Multiple devices depending on same gateway
    for i in 1..=3 {
        let device = TopologyNode {
            id: format!("device-{}", i),
            node_type: TopologyNodeType::Device,
            label: format!("Device {}", i),
            properties: Default::default(),
            state: NodeState::Online,
            observed_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        topology_store.upsert_node(&device).await?;

        let edge = TopologyEdge {
            id: format!("edge-{}-gateway", i),
            source: format!("device-{}", i),
            target: "shared-gateway".to_string(),
            edge_type: TopologyEdgeType::GatewayOf,
            provenance: Provenance::RoutingTable,
            confidence: 0.9,
            observed_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        topology_store.upsert_edge(&edge).await?;
    }

    Ok(())
}

async fn setup_topology_incomplete(topology_store: &TopologyStore) -> Result<()> {
    // Device without gateway info
    let device = TopologyNode {
        id: "device-unknown".to_string(),
        node_type: TopologyNodeType::Device,
        label: "Unknown Device".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&device).await?;

    // Another device with gateway
    let gateway = TopologyNode {
        id: "gateway-known".to_string(),
        node_type: TopologyNodeType::Gateway,
        label: "Known Gateway".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&gateway).await?;

    let device2 = TopologyNode {
        id: "device-known".to_string(),
        node_type: TopologyNodeType::Device,
        label: "Known Device".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&device2).await?;

    let edge = TopologyEdge {
        id: "edge-known-gateway".to_string(),
        source: "device-known".to_string(),
        target: "gateway-known".to_string(),
        edge_type: TopologyEdgeType::GatewayOf,
        provenance: Provenance::RoutingTable,
        confidence: 0.9,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_edge(&edge).await?;

    Ok(())
}

#[tokio::test]
async fn test_gateway_failure_multiple_dependents() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_with_gateway(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    // Insert failing observations for all 5 devices AND the gateway
    for i in 1..=5 {
        insert_failing_observations(&storage, &format!("device-{}", i), "icmp", 3, since).await;
    }
    insert_failing_observations(&storage, "gateway-1", "icmp", 3, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    assert!(!hypotheses.is_empty());
    // Gateway should be the top hypothesis
    let top = &hypotheses[0];
    assert_eq!(top.entity_id, "gateway-1");
    assert!(top.confidence > 0.8);
    assert!(top.is_root()); // Should be treated as root cause
    assert!(!top.dependency_path.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_isolated_device_failure() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_isolated_device(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    // Only the isolated device fails
    insert_failing_observations(&storage, "isolated-device", "icmp", 3, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    assert!(!hypotheses.is_empty());
    let top = &hypotheses[0];
    assert_eq!(top.entity_id, "isolated-device");
    // No upstream dependencies, so it's the root cause
    assert!(top.confidence > 0.5);
    // Should have empty dependency path since no upstream
    Ok(())
}

#[tokio::test]
async fn test_simultaneous_failures_shared_dependency() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_shared_dependency(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    // All 3 devices fail simultaneously
    for i in 1..=3 {
        insert_failing_observations(&storage, &format!("device-{}", i), "icmp", 3, since).await;
    }
    // Gateway also fails
    insert_failing_observations(&storage, "shared-gateway", "icmp", 3, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    assert!(!hypotheses.is_empty());
    // Shared gateway should be top hypothesis
    let top = &hypotheses[0];
    assert_eq!(top.entity_id, "shared-gateway");
    assert!(top.confidence > 0.7);
    Ok(())
}

#[tokio::test]
async fn test_incomplete_topology() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_incomplete(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    // Both devices fail
    insert_failing_observations(&storage, "device-unknown", "icmp", 3, since).await;
    insert_failing_observations(&storage, "device-known", "icmp", 3, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    // Should generate hypotheses for both
    assert_eq!(hypotheses.len(), 2);
    // device-known should have gateway as upstream cause
    let known_hyp = hypotheses
        .iter()
        .find(|h| h.entity_id == "device-known")
        .unwrap();
    assert!(!known_hyp.dependency_path.is_empty());
    // device-unknown should have no upstream (no gateway info)
    let unknown_hyp = hypotheses
        .iter()
        .find(|h| h.entity_id == "device-unknown")
        .unwrap();
    assert!(unknown_hyp.dependency_path.is_empty() || unknown_hyp.confidence < 0.8);
    Ok(())
}

#[tokio::test]
async fn test_no_failing_observations() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_with_gateway(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    // No failing observations inserted
    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    assert!(hypotheses.is_empty());
    Ok(())
}

#[tokio::test]
async fn test_hypothesis_evidence_structure() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_with_gateway(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    insert_failing_observations(&storage, "device-1", "icmp", 2, since).await;
    insert_failing_observations(&storage, "gateway-1", "icmp", 2, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    assert!(!hypotheses.is_empty());
    let hyp = &hypotheses[0];

    // Check hypothesis structure
    assert!(!hyp.id.is_empty());
    assert!(!hyp.entity_id.is_empty());
    assert!(!hyp.entity_type.is_empty());
    assert!(!hyp.entity_label.is_empty());
    assert!(hyp.confidence >= 0.0 && hyp.confidence <= 1.0);
    assert!(!hyp.evidence.is_empty());
    assert!(hyp.time_window_start <= hyp.time_window_end);
    assert!(!hyp.confirmed);
    assert!(hyp.confirmed_by.is_none());
    assert!(hyp.confirmed_at.is_none());

    // Check evidence structure
    for evidence in &hyp.evidence {
        assert!(!evidence.check_id.is_empty());
        assert!(!evidence.entity_id.is_empty());
        assert!(!evidence.protocol.is_empty());
        assert!(!evidence.success); // All failing
        assert!(evidence.timestamp >= since && evidence.timestamp <= until);
        assert!(evidence.weight > 0.0);
    }

    // Check dependency path
    for link in &hyp.dependency_path {
        assert!(!link.from_entity_id.is_empty());
        assert!(!link.to_entity_id.is_empty());
        assert!(!link.relationship.is_empty());
        assert!(!link.provenance.is_empty());
        assert!(link.confidence >= 0.0 && link.confidence <= 1.0);
    }
    Ok(())
}

#[tokio::test]
async fn test_confidence_scoring_direct_vs_correlation() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    // Create a chain: device -> intermediate -> gateway
    let gateway = TopologyNode {
        id: "gateway-chain".to_string(),
        node_type: TopologyNodeType::Gateway,
        label: "Gateway Chain".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&gateway).await?;

    let intermediate = TopologyNode {
        id: "intermediate".to_string(),
        node_type: TopologyNodeType::Device,
        label: "Intermediate".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&intermediate).await?;

    let device = TopologyNode {
        id: "device-chain".to_string(),
        node_type: TopologyNodeType::Device,
        label: "Device Chain".to_string(),
        properties: Default::default(),
        state: NodeState::Online,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_node(&device).await?;

    // device -> intermediate -> gateway
    let edge1 = TopologyEdge {
        id: "edge-device-intermediate".to_string(),
        source: "device-chain".to_string(),
        target: "intermediate".to_string(),
        edge_type: TopologyEdgeType::GatewayOf,
        provenance: Provenance::RoutingTable,
        confidence: 0.8,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_edge(&edge1).await?;

    let edge2 = TopologyEdge {
        id: "edge-intermediate-gateway".to_string(),
        source: "intermediate".to_string(),
        target: "gateway-chain".to_string(),
        edge_type: TopologyEdgeType::GatewayOf,
        provenance: Provenance::RoutingTable,
        confidence: 0.8,
        observed_at: Utc::now(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    topology_store.upsert_edge(&edge2).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(1);
    let until = Utc::now();

    // Only gateway fails
    insert_failing_observations(&storage, "gateway-chain", "icmp", 3, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    // Should identify gateway as root cause with high confidence
    // (direct observation evidence)
    assert!(!hypotheses.is_empty());
    let top = &hypotheses[0];
    assert_eq!(top.entity_id, "gateway-chain");
    assert!(top.confidence > 0.8); // High confidence from direct observation
    Ok(())
}

#[tokio::test]
async fn test_hypothesis_time_window() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let topology_store = Arc::new(TopologyStore::new(storage.pool.clone()));

    setup_topology_with_gateway(&topology_store).await?;

    let analyzer = RootCauseAnalyzer::new(topology_store, storage.clone());

    let since = Utc::now() - Duration::hours(2);
    let until = Utc::now() - Duration::hours(1);

    insert_failing_observations(&storage, "device-1", "icmp", 3, since).await;
    insert_failing_observations(&storage, "gateway-1", "icmp", 3, since).await;

    let hypotheses = analyzer.analyze(since, Some(until)).await?;

    assert!(!hypotheses.is_empty());
    let hyp = &hypotheses[0];

    // Time window should match the analysis window
    assert!(hyp.time_window_start >= since);
    assert!(hyp.time_window_end <= until);
    Ok(())
}
