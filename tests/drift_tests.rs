use chrono::Utc;
use netwatch::drift::{DriftDetector, DriftReport};
use netwatch::topology::{NodeState, TopologyNode, TopologyNodeType};

fn create_test_nodes() -> Vec<netwatch::topology::TopologyNode> {
    vec![
        TopologyNode {
            id: "device-eth0".to_string(),
            node_type: TopologyNodeType::Device,
            label: "eth0".to_string(),
            properties: {
                let mut props = std::collections::HashMap::new();
                props.insert("ipv4".to_string(), serde_json::json!("172.17.1.84"));
                props.insert("mac".to_string(), serde_json::json!("00:15:5d:4f:7e:c4"));
                props.insert("mtu".to_string(), serde_json::json!(1492));
                props
            },
            state: NodeState::Online,
            observed_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        TopologyNode {
            id: "iface-eth0".to_string(),
            node_type: TopologyNodeType::Interface,
            label: "eth0".to_string(),
            properties: {
                let mut props = std::collections::HashMap::new();
                props.insert("ipv4".to_string(), serde_json::json!("172.17.1.84"));
                props.insert("mac".to_string(), serde_json::json!("00:15:5d:4f:7e:c4"));
                props.insert("mtu".to_string(), serde_json::json!(1492));
                props
            },
            state: NodeState::Online,
            observed_at: Utc::now(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    ]
}

fn create_expected_inventory() -> netwatch::drift::ExpectedInventory {
    netwatch::drift::ExpectedInventory {
        devices: vec![netwatch::drift::ExpectedDevice {
            id: "device-eth0".to_string(),
            name: "eth0".to_string(),
            expected_ips: vec!["172.17.1.84".parse().unwrap()],
            expected_mac: Some("00:15:5d:4f:7e:c4".to_string()),
            expected_type: "interface".to_string(),
            site: None,
            network: None,
            vlan: None,
            tags: vec![],
            expected_interfaces: vec!["eth0".to_string()],
        }],
        interfaces: vec![netwatch::drift::ExpectedInterface {
            device_id: "device-eth0".to_string(),
            name: "eth0".to_string(),
            expected_ip: Some("172.17.1.84".parse().unwrap()),
            expected_mac: Some("00:15:5d:4f:7e:c4".to_string()),
            expected_mtu: Some(1492),
            expected_speed: Some(1000000000),
            expected_admin_status: Some(1),
            expected_oper_status: Some(1),
        }],
        routes: vec![],
        subnets: vec![],
        updated_at: chrono::Utc::now(),
        version: 1,
    }
}

#[tokio::test]
async fn test_drift_detector_no_drift() {
    let expected = create_expected_inventory();
    let detector = DriftDetector::new(expected);
    let nodes = create_test_nodes();
    let edges: Vec<netwatch::topology::TopologyEdge> = vec![];

    let report = detector.detect_drift(&nodes, &edges, None).unwrap();

    // Debug: print findings and expected/actual interface IDs
    eprintln!("Expected interface IDs: {:?}", ["iface-eth0"]);
    for node in &nodes {
        if node.node_type == netwatch::topology::TopologyNodeType::Interface {
            eprintln!("ACTUAL INTERFACE: id={}, label={}", node.id, node.label);
        }
    }
    for f in &report.findings {
        eprintln!(
            "FINDING: {:?} {:?} {:?} {:?}",
            f.drift_type, f.entity_type, f.entity_id, f.field
        );
    }

    // Should have no findings since actual matches expected
    assert_eq!(report.findings.len(), 0);
    assert_eq!(report.summary.total_findings, 0);
}

#[tokio::test]
async fn test_drift_detector_device_missing() {
    let expected = create_expected_inventory();
    let _detector = DriftDetector::new(expected);
    let nodes: Vec<netwatch::topology::TopologyNode> = vec![];
    let edges: Vec<netwatch::topology::TopologyEdge> = vec![];

    let report = DriftDetector::new(create_expected_inventory())
        .detect_drift(&nodes, &edges, None)
        .unwrap();

    // Debug: print findings
    for f in &report.findings {
        eprintln!(
            "FINDING: {:?} {:?} {:?} {:?}",
            f.drift_type, f.entity_type, f.entity_id, f.field
        );
    }

    // Should find the missing device AND the missing interface
    assert_eq!(report.findings.len(), 2);
    let device_finding = report
        .findings
        .iter()
        .find(|f| f.entity_type == "device")
        .unwrap();
    assert_eq!(
        device_finding.drift_type,
        netwatch::drift::DriftType::Removed
    );
    assert_eq!(
        device_finding.severity,
        netwatch::drift::DriftSeverity::Critical
    );
    assert_eq!(device_finding.entity_type, "device");
    assert_eq!(device_finding.entity_id, "device-eth0");

    let interface_finding = report
        .findings
        .iter()
        .find(|f| f.entity_type == "interface")
        .unwrap();
    assert_eq!(
        interface_finding.drift_type,
        netwatch::drift::DriftType::Removed
    );
    assert_eq!(
        interface_finding.severity,
        netwatch::drift::DriftSeverity::Warning
    );
    assert_eq!(interface_finding.entity_type, "interface");
    assert_eq!(interface_finding.entity_id, "iface-eth0");
}

#[tokio::test]
async fn test_drift_detector_unexpected_device() {
    let expected = netwatch::drift::ExpectedInventory::default(); // No expected devices
    let _detector = DriftDetector::new(expected);
    let nodes = vec![netwatch::topology::TopologyNode {
        id: "device-unknown".to_string(),
        node_type: netwatch::topology::TopologyNodeType::Device,
        label: "unknown-device".to_string(),
        properties: std::collections::HashMap::new(),
        state: netwatch::topology::NodeState::Online,
        observed_at: chrono::Utc::now(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }];
    let edges: Vec<netwatch::topology::TopologyEdge> = vec![];

    let report = DriftDetector::new(netwatch::drift::ExpectedInventory::default())
        .detect_drift(&nodes, &edges, None)
        .unwrap();

    // Should find the unexpected device
    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.drift_type, netwatch::drift::DriftType::Added);
    assert_eq!(finding.severity, netwatch::drift::DriftSeverity::Warning);
    assert_eq!(finding.entity_type, "device");
}

#[tokio::test]
async fn test_drift_report_markdown() {
    let report = DriftReport {
        id: "test-report".to_string(),
        generated_at: chrono::Utc::now(),
        expected_inventory_version: 1,
        actual_snapshot_id: None,
        findings: vec![],
        summary: Default::default(),
        format_version: 1,
    };

    let markdown = report.to_markdown();
    assert!(markdown.contains("# Drift Report"));
    assert!(markdown.contains("Report ID"));
}

#[tokio::test]
async fn test_drift_report_json() {
    let report = DriftReport {
        id: "test-report".to_string(),
        generated_at: chrono::Utc::now(),
        expected_inventory_version: 1,
        actual_snapshot_id: None,
        findings: vec![],
        summary: Default::default(),
        format_version: 1,
    };

    let json = report.to_json().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["id"], "test-report");
}
