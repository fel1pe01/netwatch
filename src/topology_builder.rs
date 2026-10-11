use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use uuid::Uuid;

use crate::discovery::interfaces::discover_interfaces;
use crate::discovery::routing::discover_routes;
use crate::topology::{
    NodeState, Provenance, TopologyDiff, TopologyEdge, TopologyGraph, TopologyNode,
    TopologyNodeType, TopologySnapshot,
};
use crate::topology_store::TopologyStore;

pub struct TopologyBuilder {
    store: Arc<TopologyStore>,
    graph: TopologyGraph,
}

impl Clone for TopologyBuilder {
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            graph: self.graph.clone(),
        }
    }
}

impl TopologyBuilder {
    pub fn new(store: Arc<TopologyStore>) -> Self {
        Self {
            store,
            graph: TopologyGraph::new(),
        }
    }

    pub async fn build_from_discovery(&mut self) -> Result<TopologySnapshot> {
        // Load existing graph from storage
        self.load_from_storage().await?;

        // Discover local interfaces
        let interfaces = discover_interfaces().await?;
        let routes = discover_routes().await?;

        // Build topology from discovered data
        self.process_interfaces(&interfaces).await?;
        self.process_routes(&routes).await?;

        // Persist all changes
        self.persist_all().await?;

        // Create and return snapshot
        let snapshot = self.graph.to_snapshot();
        Ok(snapshot)
    }

    pub async fn build_from_snmp(
        &mut self,
        snmp_data: &[SnmpDeviceData],
    ) -> Result<TopologySnapshot> {
        // Load existing graph from storage
        self.load_from_storage().await?;

        // Process SNMP data
        self.process_snmp_devices(snmp_data).await?;

        // Persist all changes
        self.persist_all().await?;

        let snapshot = self.graph.to_snapshot();
        Ok(snapshot)
    }

    async fn load_from_storage(&mut self) -> Result<()> {
        let nodes = self.store.list_nodes().await?;
        let edges = self.store.list_edges().await?;

        for node in nodes {
            self.graph.add_node(node);
        }
        for edge in edges {
            self.graph.add_edge(edge)?;
        }
        Ok(())
    }

    async fn process_interfaces(
        &mut self,
        interfaces: &[crate::discovery::interfaces::LocalInterface],
    ) -> Result<()> {
        for iface in interfaces {
            // Create or update device node for this interface
            let device_id = format!("device-{}", iface.name);
            let device_node = TopologyNode {
                id: device_id.clone(),
                node_type: TopologyNodeType::Device,
                label: iface.name.clone(),
                properties: {
                    let mut props = HashMap::new();
                    if let Some(mac) = &iface.mac {
                        props.insert("mac".to_string(), serde_json::Value::String(mac.clone()));
                    }
                    if let Some(mtu) = iface.mtu {
                        props.insert("mtu".to_string(), serde_json::Value::Number(mtu.into()));
                    }
                    props.insert(
                        "evidence".to_string(),
                        serde_json::Value::String("local_interface".to_string()),
                    );
                    props
                },
                state: match iface.oper_state.as_deref() {
                    Some("up") => NodeState::Online,
                    Some("down") => NodeState::Offline,
                    Some("unknown") => NodeState::Unknown,
                    _ => NodeState::Unknown,
                },
                observed_at: Utc::now(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            // Create interface node
            let iface_id = format!("iface-{}", iface.name);
            let mut props = HashMap::new();
            if let Some(mac) = &iface.mac {
                props.insert("mac".to_string(), serde_json::Value::String(mac.clone()));
            }
            if let Some(mtu) = iface.mtu {
                props.insert("mtu".to_string(), serde_json::Value::Number(mtu.into()));
            }
            if let Some(ipv4) = &iface.ipv4 {
                props.insert("ipv4".to_string(), serde_json::Value::String(ipv4.clone()));
            }
            if let Some(ipv6) = &iface.ipv6 {
                props.insert("ipv6".to_string(), serde_json::Value::String(ipv6.clone()));
            }
            props.insert(
                "evidence".to_string(),
                serde_json::Value::String("local_interface".to_string()),
            );

            let iface_node = TopologyNode {
                id: iface_id.clone(),
                node_type: TopologyNodeType::Interface,
                label: iface.name.clone(),
                properties: props,
                state: match iface.oper_state.as_deref() {
                    Some("up") => NodeState::Online,
                    Some("down") => NodeState::Offline,
                    Some("unknown") => NodeState::Unknown,
                    _ => NodeState::Unknown,
                },
                observed_at: Utc::now(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            // Add nodes to graph
            self.graph.add_node(device_node);
            self.graph.add_node(iface_node);

            // Add contains edge: device contains interface
            let contains_edge = TopologyEdge {
                id: Uuid::new_v4().to_string(),
                source: device_id,
                target: iface_id.clone(),
                edge_type: crate::topology::TopologyEdgeType::Contains,
                provenance: Provenance::Discovery,
                confidence: 1.0,
                observed_at: Utc::now(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            self.graph.add_edge(contains_edge)?;

            // Process IPs - create subnet nodes
            self.process_interface_ips(&iface_id, iface).await?;
        }
        Ok(())
    }

    async fn process_interface_ips(
        &mut self,
        iface_id: &str,
        iface: &crate::discovery::interfaces::LocalInterface,
    ) -> Result<()> {
        // Process IPv4
        if let Some(ipv4_str) = &iface.ipv4 {
            if let Ok(ip) = ipv4_str.parse::<Ipv4Addr>() {
                // Create subnet node (assuming /24 for now)
                let subnet_str = format!(
                    "{}/24",
                    &ip.to_string()[..ip.to_string().rfind('.').unwrap_or(0) + 1].to_string()
                );
                let subnet_id = format!("subnet-{}", subnet_str.replace(['.', '/'], "-"));

                let subnet_node = TopologyNode {
                    id: subnet_id.clone(),
                    node_type: TopologyNodeType::Subnet,
                    label: subnet_str.clone(),
                    properties: {
                        let mut props = HashMap::new();
                        props.insert("cidr".to_string(), serde_json::Value::String(subnet_str));
                        props.insert(
                            "evidence".to_string(),
                            serde_json::Value::String("local_interface".to_string()),
                        );
                        props
                    },
                    state: NodeState::Online,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                self.graph.add_node(subnet_node);

                // Add edge: interface connected to subnet
                let edge = TopologyEdge {
                    id: Uuid::new_v4().to_string(),
                    source: iface_id.to_string(),
                    target: subnet_id,
                    edge_type: crate::topology::TopologyEdgeType::ConnectedTo,
                    provenance: Provenance::Discovery,
                    confidence: 0.9,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.graph.add_edge(edge)?;
            }
        }

        // Process IPv6
        if let Some(ipv6_str) = &iface.ipv6 {
            if let Ok(_ip) = ipv6_str.parse::<Ipv6Addr>() {
                // Create subnet node for IPv6 (assuming /64)
                let subnet_str = format!(
                    "{}/64",
                    &ipv6_str[..ipv6_str.rfind(':').unwrap_or(0) + 1].to_string()
                );
                let subnet_id = format!("subnet-{}", subnet_str.replace([':', '/'], "-"));

                let subnet_node = TopologyNode {
                    id: subnet_id.clone(),
                    node_type: TopologyNodeType::Subnet,
                    label: subnet_str.clone(),
                    properties: {
                        let mut props = HashMap::new();
                        props.insert("cidr".to_string(), serde_json::Value::String(subnet_str));
                        props.insert(
                            "evidence".to_string(),
                            serde_json::Value::String("local_interface".to_string()),
                        );
                        props
                    },
                    state: NodeState::Online,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };

                self.graph.add_node(subnet_node);

                // Add edge: interface connected to subnet
                let edge = TopologyEdge {
                    id: Uuid::new_v4().to_string(),
                    source: iface_id.to_string(),
                    target: subnet_id,
                    edge_type: crate::topology::TopologyEdgeType::ConnectedTo,
                    provenance: Provenance::Discovery,
                    confidence: 0.9,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.graph.add_edge(edge)?;
            }
        }
        Ok(())
    }

    async fn process_routes(
        &mut self,
        routes: &[crate::discovery::routing::RouteEntry],
    ) -> Result<()> {
        for route in routes {
            // Skip default route for now
            if route.destination == "0.0.0.0" || route.destination == "::" {
                continue;
            }

            // Find gateway node if exists
            if let Some(gateway_ip) = &route.gateway {
                let gateway_id = format!("gateway-{}", gateway_ip);

                // Create or update gateway node
                let gateway_node = crate::topology::TopologyNode {
                    id: gateway_id.clone(),
                    node_type: crate::topology::TopologyNodeType::Gateway,
                    label: gateway_ip.clone(),
                    properties: {
                        let mut props = std::collections::HashMap::new();
                        props.insert(
                            "ip".to_string(),
                            serde_json::Value::String(gateway_ip.clone()),
                        );
                        props.insert(
                            "evidence".to_string(),
                            serde_json::Value::String("routing_table".to_string()),
                        );
                        props
                    },
                    state: crate::topology::NodeState::Online,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.graph.add_node(gateway_node);

                // Find interface node
                let iface_id = format!("iface-{}", route.interface);

                // Add edge: interface routes through gateway
                let edge = crate::topology::TopologyEdge {
                    id: Uuid::new_v4().to_string(),
                    source: iface_id,
                    target: gateway_id,
                    edge_type: crate::topology::TopologyEdgeType::RoutesThrough,
                    provenance: Provenance::RoutingTable,
                    confidence: 0.95,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.graph.add_edge(edge)?;
            }
        }
        Ok(())
    }

    async fn process_snmp_devices(&mut self, snmp_data: &[SnmpDeviceData]) -> Result<()> {
        for device in snmp_data {
            let device_id = format!("device-{}", device.address);

            let device_node = crate::topology::TopologyNode {
                id: device_id.clone(),
                node_type: crate::topology::TopologyNodeType::Device,
                label: device
                    .sys_name
                    .clone()
                    .unwrap_or_else(|| device.address.to_string()),
                properties: {
                    let mut props = HashMap::new();
                    props.insert(
                        "sys_descr".to_string(),
                        serde_json::Value::String(device.sys_descr.clone()),
                    );
                    props.insert(
                        "sys_object_id".to_string(),
                        serde_json::Value::String(device.sys_object_id.clone()),
                    );
                    props.insert(
                        "sys_up_time".to_string(),
                        serde_json::Value::Number(device.sys_up_time.into()),
                    );
                    if let Some(name) = &device.sys_name {
                        props.insert(
                            "sys_name".to_string(),
                            serde_json::Value::String(name.clone()),
                        );
                    }
                    if let Some(location) = &device.sys_location {
                        props.insert(
                            "sys_location".to_string(),
                            serde_json::Value::String(location.clone()),
                        );
                    }
                    props.insert(
                        "evidence".to_string(),
                        serde_json::Value::String("snmp".to_string()),
                    );
                    props
                },
                state: crate::topology::NodeState::Online,
                observed_at: Utc::now(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            self.graph.add_node(device_node);

            // Add interfaces from SNMP
            for iface in &device.interfaces {
                let iface_id = format!("iface-{}-{}", device.address, iface.if_index);

                let iface_node = crate::topology::TopologyNode {
                    id: iface_id.clone(),
                    node_type: crate::topology::TopologyNodeType::Interface,
                    label: iface
                        .if_name
                        .clone()
                        .unwrap_or_else(|| format!("if{}", iface.if_index)),
                    properties: {
                        let mut props = HashMap::new();
                        props.insert(
                            "if_descr".to_string(),
                            serde_json::Value::String(iface.if_descr.clone()),
                        );
                        props.insert(
                            "if_type".to_string(),
                            serde_json::Value::Number(iface.if_type.into()),
                        );
                        props.insert(
                            "if_mtu".to_string(),
                            serde_json::Value::Number(iface.if_mtu.into()),
                        );
                        props.insert(
                            "if_speed".to_string(),
                            serde_json::Value::Number(iface.if_speed.into()),
                        );
                        props.insert(
                            "if_admin_status".to_string(),
                            serde_json::Value::Number(iface.if_admin_status.into()),
                        );
                        props.insert(
                            "if_oper_status".to_string(),
                            serde_json::Value::Number(iface.if_oper_status.into()),
                        );
                        props.insert(
                            "if_high_speed".to_string(),
                            iface
                                .if_high_speed
                                .map(|v| serde_json::Value::Number(v.into()))
                                .unwrap_or(serde_json::Value::Null),
                        );
                        props.insert(
                            "evidence".to_string(),
                            serde_json::Value::String("snmp".to_string()),
                        );
                        props
                    },
                    state: match iface.if_oper_status {
                        1 => NodeState::Online,   // up
                        2 => NodeState::Offline,  // down
                        3 => NodeState::Degraded, // testing
                        _ => NodeState::Unknown,
                    },
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.graph.add_node(iface_node);

                // Add contains edge: device contains interface
                let contains_edge = crate::topology::TopologyEdge {
                    id: Uuid::new_v4().to_string(),
                    source: device_id.clone(),
                    target: iface_id.clone(),
                    edge_type: crate::topology::TopologyEdgeType::Contains,
                    provenance: Provenance::Snmp,
                    confidence: 1.0,
                    observed_at: Utc::now(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                self.graph.add_edge(contains_edge)?;
            }
        }
        Ok(())
    }

    async fn persist_all(&mut self) -> Result<()> {
        // Persist nodes
        for node in self.graph.nodes() {
            self.store.upsert_node(node).await?;
        }

        // Persist edges
        for edge in self.graph.edges() {
            self.store.upsert_edge(edge).await?;
        }
        Ok(())
    }

    // Create and persist a snapshot
    pub async fn create_snapshot(&mut self) -> Result<TopologySnapshot> {
        let snapshot = self.graph.to_snapshot();
        self.store.create_snapshot(&snapshot).await?;
        Ok(snapshot)
    }

    // Compare with another snapshot
    pub async fn diff_with_latest(&mut self) -> Result<TopologyDiff> {
        if let Some(latest_snapshot) = self.store.get_latest_snapshot().await? {
            // Reconstruct graph from snapshot
            let mut other_graph = TopologyGraph::new();
            for node in &latest_snapshot.nodes {
                other_graph.add_node(node.clone());
            }
            for edge in &latest_snapshot.edges {
                other_graph.add_edge(edge.clone())?;
            }

            let diff = self.graph.diff(&other_graph);
            Ok(diff)
        } else {
            // No previous snapshot
            let diff = crate::topology::TopologyDiff {
                added_nodes: self.graph.nodes().into_iter().cloned().collect(),
                removed_nodes: Vec::new(),
                added_edges: self.graph.edges().into_iter().cloned().collect(),
                removed_edges: Vec::new(),
                changed_nodes: Vec::new(),
                changed_edges: Vec::new(),
            };
            Ok(diff)
        }
    }

    // Export to Mermaid
    pub fn export_mermaid(&self) -> String {
        let mut output = String::from("graph TD\n");

        // Add nodes
        for node in self.graph.nodes() {
            let _shape = match node.node_type {
                crate::topology::TopologyNodeType::Device => "box",
                crate::topology::TopologyNodeType::Interface => "circle",
                crate::topology::TopologyNodeType::Subnet => "hexagon",
                crate::topology::TopologyNodeType::Gateway => "diamond",
            };
            let color = match node.state {
                NodeState::Online => "#22c55e",
                NodeState::Degraded => "#f59e0b",
                NodeState::Offline => "#ef4444",
                NodeState::Unknown => "#9ca3af",
            };
            let node_id = node.id.replace('-', "_");
            let node_line = format!(
                "    {}[{}::{}]:::style{}\n",
                node_id, node.label, node.node_type, node_id
            );
            let class_line = format!(
                "    classDef style{} fill:{},stroke:#333,stroke-width:2px;\n",
                node_id, color
            );
            output.push_str(&node_line);
            output.push_str(&class_line);
        }

        // Add edges
        for edge in self.graph.edges() {
            let edge_line = format!(
                "    {} -->|{}| {}\n",
                edge.source.replace('-', "_"),
                edge.edge_type,
                edge.target.replace('-', "_")
            );
            output.push_str(&edge_line);
        }

        output
    }

    // Export to Graphviz DOT
    pub fn export_dot(&self) -> String {
        let mut output =
            String::from("digraph topology {\n    rankdir=LR;\n    node [fontname=\"Arial\"];\n");

        // Add nodes
        for node in self.graph.nodes() {
            let shape = match node.node_type {
                crate::topology::TopologyNodeType::Device => "box",
                crate::topology::TopologyNodeType::Interface => "circle",
                crate::topology::TopologyNodeType::Subnet => "hexagon",
                crate::topology::TopologyNodeType::Gateway => "diamond",
            };
            let color = match node.state {
                NodeState::Online => "green",
                NodeState::Degraded => "orange",
                NodeState::Offline => "red",
                NodeState::Unknown => "gray",
            };
            output.push_str(&format!(
                "    \"{}\" [label=\"{} ({})\", shape={}, style=filled, fillcolor={}];\n",
                node.id, node.label, node.node_type, shape, color
            ));
        }

        // Add edges
        for edge in self.graph.edges() {
            output.push_str(&format!(
                "    \"{}\" -> \"{}\" [label=\"{}\", color=blue];\n",
                edge.source, edge.target, edge.edge_type
            ));
        }

        output.push_str("}\n");
        output
    }

    // Export to JSON
    pub fn export_json(&self) -> Result<String> {
        let snapshot = self.graph.to_snapshot();
        Ok(serde_json::to_string_pretty(&snapshot)?)
    }
}

// SNMP device data structure for topology building
#[derive(Debug, Clone)]
pub struct SnmpDeviceData {
    pub address: std::net::IpAddr,
    pub sys_descr: String,
    pub sys_object_id: String,
    pub sys_up_time: u64,
    pub sys_contact: Option<String>,
    pub sys_name: Option<String>,
    pub sys_location: Option<String>,
    pub sys_services: u32,
    pub interfaces: Vec<SnmpInterfaceData>,
}

#[derive(Debug, Clone)]
pub struct SnmpInterfaceData {
    pub if_index: u32,
    pub if_name: Option<String>,
    pub if_descr: String,
    pub if_type: u32,
    pub if_mtu: u32,
    pub if_speed: u64,
    pub if_admin_status: u32,
    pub if_oper_status: u32,
    pub if_in_octets: u64,
    pub if_out_octets: u64,
    pub if_high_speed: Option<u64>,
}

impl Default for SnmpDeviceData {
    fn default() -> Self {
        Self {
            address: "0.0.0.0".parse().unwrap(),
            sys_descr: String::new(),
            sys_object_id: String::new(),
            sys_up_time: 0,
            sys_contact: None,
            sys_name: None,
            sys_location: None,
            sys_services: 0,
            interfaces: Vec::new(),
        }
    }
}
