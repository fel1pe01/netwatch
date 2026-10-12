use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use uuid::Uuid;

use crate::topology::TopologyNode;

/// Expected inventory model for drift detection
/// Defines what devices/interfaces/routes are expected to exist
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExpectedInventory {
    pub devices: Vec<ExpectedDevice>,
    pub interfaces: Vec<ExpectedInterface>,
    pub routes: Vec<ExpectedRoute>,
    pub subnets: Vec<ExpectedSubnet>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedDevice {
    pub id: String,
    pub name: String,
    pub expected_ips: Vec<IpAddr>,
    pub expected_mac: Option<String>,
    pub expected_type: String,
    pub site: Option<String>,
    pub network: Option<String>,
    pub vlan: Option<u16>,
    pub tags: Vec<String>,
    pub expected_interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedInterface {
    pub device_id: String,
    pub name: String,
    pub expected_ip: Option<IpAddr>,
    pub expected_mac: Option<String>,
    pub expected_mtu: Option<u32>,
    pub expected_speed: Option<u64>,
    pub expected_admin_status: Option<u32>,
    pub expected_oper_status: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedRoute {
    pub destination: String,
    pub gateway: Option<IpAddr>,
    pub interface: String,
    pub metric: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedSubnet {
    pub cidr: String,
    pub gateway: Option<IpAddr>,
    pub vlan: Option<u16>,
    pub site: Option<String>,
}

/// Represents a single drift finding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftFinding {
    pub id: String,
    pub drift_type: DriftType,
    pub severity: DriftSeverity,
    pub entity_type: String,
    pub entity_id: String,
    pub entity_name: String,
    pub field: String,
    pub expected: serde_json::Value,
    pub actual: serde_json::Value,
    pub detected_at: DateTime<Utc>,
    pub confidence: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DriftType {
    Added,
    Removed,
    Modified,
    Unexpected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DriftSeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for DriftType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriftType::Added => write!(f, "added"),
            DriftType::Removed => write!(f, "removed"),
            DriftType::Modified => write!(f, "modified"),
            DriftType::Unexpected => write!(f, "unexpected"),
        }
    }
}

impl std::fmt::Display for DriftSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriftSeverity::Info => write!(f, "info"),
            DriftSeverity::Warning => write!(f, "warning"),
            DriftSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Complete drift report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub id: String,
    pub generated_at: chrono::DateTime<Utc>,
    pub expected_inventory_version: u64,
    pub actual_snapshot_id: Option<String>,
    pub findings: Vec<DriftFinding>,
    pub summary: DriftSummary,
    pub format_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DriftSummary {
    pub total_findings: usize,
    pub by_type: HashMap<String, usize>,
    pub by_severity: HashMap<String, usize>,
    pub by_entity_type: HashMap<String, usize>,
    pub new_devices: usize,
    pub removed_devices: usize,
    pub modified_entities: usize,
}

/// Drift detector - compares expected inventory with actual topology
pub struct DriftDetector {
    expected_inventory: ExpectedInventory,
}

impl DriftDetector {
    pub fn new(expected_inventory: ExpectedInventory) -> Self {
        Self { expected_inventory }
    }

    pub fn set_expected_inventory(&mut self, inventory: ExpectedInventory) {
        self.expected_inventory = inventory;
    }

    pub fn get_expected_inventory(&self) -> &ExpectedInventory {
        &self.expected_inventory
    }

    /// Compare actual topology nodes with expected inventory
    /// Returns a drift report with all findings
    pub fn detect_drift(
        &self,
        actual_nodes: &[crate::topology::TopologyNode],
        _actual_edges: &[crate::topology::TopologyEdge],
        _snapshot_id: Option<String>,
    ) -> Result<DriftReport> {
        let mut actual_devices = std::collections::HashMap::new();
        let mut actual_interfaces = std::collections::HashMap::new();
        let mut actual_subnets = std::collections::HashMap::new();
        let mut actual_gateways = std::collections::HashMap::new();

        // Index actual nodes by type
        for node in actual_nodes {
            match node.node_type {
                crate::topology::TopologyNodeType::Device => {
                    actual_devices.insert(node.id.clone(), node);
                }
                crate::topology::TopologyNodeType::Interface => {
                    actual_interfaces.insert(node.id.clone(), node);
                }
                crate::topology::TopologyNodeType::Subnet => {
                    actual_subnets.insert(node.id.clone(), node);
                }
                crate::topology::TopologyNodeType::Gateway => {
                    actual_gateways.insert(node.id.clone(), node);
                }
                crate::topology::TopologyNodeType::Service => {
                    // Services are not currently compared for drift
                }
            }
        }

        let mut findings: Vec<DriftFinding> = Vec::new();
        let mut expected_device_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut actual_device_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut expected_interface_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Check devices
        for expected_device in &self.expected_inventory.devices {
            expected_device_ids.insert(expected_device.id.clone());
            if let Some(actual_device) = actual_devices.get(&expected_device.id) {
                // Device exists in both - check for modifications
                #[allow(clippy::needless_borrow)]
                self.compare_device(&expected_device, actual_device, &mut findings)?;
            } else {
                // Device in expected but not in actual - REMOVED
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Removed,
                    severity: DriftSeverity::Critical,
                    entity_type: "device".to_string(),
                    entity_id: expected_device.id.clone(),
                    entity_name: expected_device.name.clone(),
                    field: "existence".to_string(),
                    expected: serde_json::json!({"present": true}),
                    actual: serde_json::json!({"present": false}),
                    detected_at: chrono::Utc::now(),
                    confidence: 1.0,
                    description: format!(
                        "Expected device '{}' not found in discovery",
                        expected_device.name
                    ),
                });
            }
        }

        // Check for unexpected devices (in actual but not expected)
        for actual_device in actual_devices.values() {
            actual_device_ids.insert(actual_device.id.clone());
            if !expected_device_ids.contains(&actual_device.id) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Added,
                    severity: DriftSeverity::Warning,
                    entity_type: "device".to_string(),
                    entity_id: actual_device.id.clone(),
                    entity_name: actual_device.label.clone(),
                    field: "existence".to_string(),
                    expected: serde_json::json!({"present": false}),
                    actual: serde_json::json!({"present": true}),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.9,
                    description: format!("Unexpected device '{}' discovered", actual_device.label),
                });
            }
        }

        // Check interfaces
        for expected_interface in &self.expected_inventory.interfaces {
            let iface_key = format!("iface-{}", expected_interface.name);
            expected_interface_ids.insert(iface_key.clone());
            if let Some(actual_interface) = actual_interfaces.get(&iface_key) {
                // Interface exists - check for modifications
                #[allow(clippy::needless_borrow)]
                self.compare_interface(&expected_interface, actual_interface, &mut findings)?;
            } else {
                // Expected interface not found
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Removed,
                    severity: DriftSeverity::Warning,
                    entity_type: "interface".to_string(),
                    entity_id: format!("iface-{}", expected_interface.name),
                    entity_name: expected_interface.name.clone(),
                    field: "existence".to_string(),
                    expected: serde_json::json!({"present": true}),
                    actual: serde_json::json!({"present": false}),
                    detected_at: chrono::Utc::now(),
                    confidence: 1.0,
                    description: format!(
                        "Expected interface '{}' not found",
                        expected_interface.name
                    ),
                });
            }
        }

        // Check for unexpected interfaces (in actual but not in expected)
        for actual_interface in actual_interfaces.values() {
            let key = actual_interface.id.clone();
            if !expected_interface_ids.contains(&key) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Added,
                    severity: DriftSeverity::Info,
                    entity_type: "interface".to_string(),
                    entity_id: actual_interface.id.clone(),
                    entity_name: actual_interface.label.clone(),
                    field: "existence".to_string(),
                    expected: serde_json::json!({"present": false}),
                    actual: serde_json::json!({"present": true}),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.8,
                    description: format!(
                        "Unexpected interface '{}' discovered",
                        actual_interface.label
                    ),
                });
            }
        }

        // Check subnets
        for expected_subnet in &self.expected_inventory.subnets {
            let expected_key = format!("subnet-{}", expected_subnet.cidr.replace(['.', '/'], "-"));
            if let Some(_actual_subnet) = actual_subnets.get(&expected_key) {
                // Subnet exists - could check for modifications
            } else {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Removed,
                    severity: DriftSeverity::Warning,
                    entity_type: "subnet".to_string(),
                    entity_id: expected_key.clone(),
                    entity_name: expected_subnet.cidr.clone(),
                    field: "existence".to_string(),
                    expected: serde_json::json!({"present": true}),
                    actual: serde_json::json!({"present": false}),
                    detected_at: chrono::Utc::now(),
                    confidence: 1.0,
                    description: format!("Expected subnet '{}' not found", expected_subnet.cidr),
                });
            }
        }

        // Check for unexpected subnets
        for actual_subnet in actual_subnets.values() {
            let actual_key = actual_subnet.id.clone();
            if !self
                .expected_inventory
                .subnets
                .iter()
                .any(|s| format!("subnet-{}", s.cidr.replace(['.', '/'], "-")) == actual_key)
            {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Added,
                    severity: DriftSeverity::Info,
                    entity_type: "subnet".to_string(),
                    entity_id: actual_key,
                    entity_name: actual_subnet.label.clone(),
                    field: "existence".to_string(),
                    expected: serde_json::json!({"present": false}),
                    actual: serde_json::json!({"present": true}),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.8,
                    description: format!("Unexpected subnet '{}' discovered", actual_subnet.label),
                });
            }
        }

        // Build summary
        let mut summary = DriftSummary::default();
        for finding in &findings {
            summary.total_findings += 1;
            *summary
                .by_type
                .entry(finding.drift_type.to_string())
                .or_insert(0) += 1;
            *summary
                .by_severity
                .entry(finding.severity.to_string())
                .or_insert(0) += 1;
            *summary
                .by_entity_type
                .entry(finding.entity_type.clone())
                .or_insert(0) += 1;

            match finding.drift_type {
                DriftType::Added => {
                    if finding.entity_type == "device" {
                        summary.new_devices += 1;
                    }
                }
                DriftType::Removed => {
                    if finding.entity_type == "device" {
                        summary.removed_devices += 1;
                    }
                }
                DriftType::Modified => {
                    summary.modified_entities += 1;
                }
                _ => {}
            }
        }

        let report = DriftReport {
            id: Uuid::new_v4().to_string(),
            generated_at: chrono::Utc::now(),
            expected_inventory_version: self.expected_inventory.version,
            actual_snapshot_id: None,
            findings,
            summary,
            format_version: 1,
        };

        Ok(report)
    }

    #[allow(clippy::needless_borrow)]
    fn compare_device(
        &self,
        expected: &ExpectedDevice,
        actual: &crate::topology::TopologyNode,
        findings: &mut Vec<DriftFinding>,
    ) -> Result<()> {
        // Check IP addresses
        let actual_ips = self.extract_ips_from_node(actual);
        for expected_ip in &expected.expected_ips {
            if !actual_ips.contains(expected_ip) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Modified,
                    severity: DriftSeverity::Warning,
                    entity_type: "device".to_string(),
                    entity_id: actual.id.clone(),
                    entity_name: actual.label.clone(),
                    field: "ipv4".to_string(),
                    expected: serde_json::json!(expected_ip.to_string()),
                    actual: serde_json::json!(actual_ips
                        .iter()
                        .map(|ip| ip.to_string())
                        .collect::<Vec<_>>()),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.9,
                    description: format!(
                        "Device '{}' missing expected IP {}",
                        actual.label, expected_ip
                    ),
                });
            }
        }

        // Check MAC
        if let Some(expected_mac) = &expected.expected_mac {
            let actual_mac = actual.properties.get("mac").and_then(|v| v.as_str());
            if actual_mac != Some(expected_mac.as_str()) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Modified,
                    severity: DriftSeverity::Warning,
                    entity_type: "device".to_string(),
                    entity_id: actual.id.clone(),
                    entity_name: actual.label.clone(),
                    field: "mac".to_string(),
                    expected: serde_json::json!(expected_mac),
                    actual: serde_json::json!(actual_mac.unwrap_or("unknown")),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.9,
                    description: format!("Device '{}' MAC changed", actual.label),
                });
            }
        }

        Ok(())
    }

    #[allow(clippy::needless_borrow)]
    fn compare_interface(
        &self,
        expected: &ExpectedInterface,
        actual: &TopologyNode,
        findings: &mut Vec<DriftFinding>,
    ) -> Result<()> {
        // Check IP
        if let Some(expected_ip) = &expected.expected_ip {
            let actual_ip = actual.properties.get("ipv4").and_then(|v| v.as_str());
            if actual_ip != Some(expected_ip.to_string().as_str()) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Modified,
                    severity: DriftSeverity::Warning,
                    entity_type: "interface".to_string(),
                    entity_id: actual.id.clone(),
                    entity_name: actual.label.clone(),
                    field: "ipv4".to_string(),
                    expected: serde_json::json!(expected_ip.to_string()),
                    actual: serde_json::json!(actual_ip.unwrap_or("none")),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.9,
                    description: format!("Interface '{}' IP changed", actual.label),
                });
            }
        }

        // Check MAC
        if let Some(expected_mac) = &expected.expected_mac {
            let actual_mac = actual.properties.get("mac").and_then(|v| v.as_str());
            if actual_mac != Some(expected_mac.as_str()) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Modified,
                    severity: DriftSeverity::Warning,
                    entity_type: "interface".to_string(),
                    entity_id: actual.id.clone(),
                    entity_name: actual.label.clone(),
                    field: "mac".to_string(),
                    expected: serde_json::json!(expected_mac),
                    actual: serde_json::json!(actual_mac.unwrap_or("unknown")),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.9,
                    description: format!("Interface '{}' MAC changed", actual.label),
                });
            }
        }

        // Check MTU
        if let Some(expected_mtu) = expected.expected_mtu {
            let actual_mtu = actual
                .properties
                .get("mtu")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32);
            if actual_mtu != Some(expected_mtu) {
                findings.push(DriftFinding {
                    id: Uuid::new_v4().to_string(),
                    drift_type: DriftType::Modified,
                    severity: DriftSeverity::Info,
                    entity_type: "interface".to_string(),
                    entity_id: actual.id.clone(),
                    entity_name: actual.label.clone(),
                    field: "mtu".to_string(),
                    expected: serde_json::json!(expected_mtu),
                    actual: serde_json::json!(actual_mtu.unwrap_or(0)),
                    detected_at: chrono::Utc::now(),
                    confidence: 0.9,
                    description: format!("Interface '{}' MTU changed", actual.label),
                });
            }
        }

        Ok(())
    }

    fn extract_ips_from_node(&self, node: &TopologyNode) -> Vec<IpAddr> {
        let mut ips = Vec::new();
        if let Some(ipv4) = node.properties.get("ipv4") {
            if let Some(ip_str) = ipv4.as_str() {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    ips.push(ip);
                }
            }
        }
        if let Some(ipv6) = node.properties.get("ipv6") {
            if let Some(ip_str) = ipv6.as_str() {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    ips.push(ip);
                }
            }
        }
        ips
    }
}

impl DriftReport {
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    #[allow(clippy::useless_format, clippy::to_string_in_format_args)]
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Drift Report\n\n");
        md.push_str(&format!("- **Report ID**: {}\n", self.id));
        md.push_str(&format!(
            "- **Generated**: {}\n",
            self.generated_at.to_rfc3339()
        ));
        md.push_str(&format!(
            "- **Expected Inventory Version**: {}\n",
            self.expected_inventory_version
        ));
        if let Some(snapshot_id) = &self.actual_snapshot_id {
            md.push_str(&format!("- **Snapshot ID**: {}\n", snapshot_id));
        }
        md.push_str("\n## Summary\n\n");
        md.push_str(&format!(
            "- **Total Findings**: {}\n",
            self.summary.total_findings
        ));
        md.push_str(&format!(
            "- **New Devices**: {}\n",
            self.summary.new_devices
        ));
        md.push_str(&format!(
            "- **Removed Devices**: {}\n",
            self.summary.removed_devices
        ));
        md.push_str(&format!(
            "- **Modified Entities**: {}\n",
            self.summary.modified_entities
        ));
        md.push_str("\n### By Type\n");
        for (dtype, count) in &self.summary.by_type {
            md.push_str(&format!("- {}: {}\n", dtype, count));
        }
        md.push_str("\n### By Severity\n");
        for (sev, count) in &self.summary.by_severity {
            md.push_str(&format!("- {}: {}\n", sev, count));
        }
        md.push_str("\n### By Entity Type\n");
        for (etype, count) in &self.summary.by_entity_type {
            md.push_str(&format!("- {}: {}\n", etype, count));
        }

        if !self.findings.is_empty() {
            md.push_str("\n## Findings\n\n");
            for finding in &self.findings {
                md.push_str(&format!(
                    "### {} - {} ({})\n\n",
                    finding.drift_type.to_string().to_uppercase(),
                    finding.entity_name,
                    finding.severity
                ));
                md.push_str(&format!(
                    "- **Entity**: {} ({})\n",
                    finding.entity_type, finding.entity_id
                ));
                md.push_str(&format!("- **Field**: {}\n", finding.field));
                md.push_str(&format!("- **Expected**: {}\n", finding.expected));
                md.push_str(&format!("- **Actual**: {}\n", finding.actual));
                md.push_str(&format!(
                    "- **Detected**: {}\n",
                    finding.detected_at.to_rfc3339()
                ));
                md.push_str(&format!(
                    "- **Confidence**: {:.0}%\n",
                    finding.confidence * 100.0
                ));
                md.push_str(&format!("- **Description**: {}\n\n", finding.description));
            }
        }

        md
    }
}
