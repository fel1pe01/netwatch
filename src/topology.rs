use anyhow::Result;
use chrono::{DateTime, Utc};
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TopologyNodeType {
    Device,
    Interface,
    Subnet,
    Gateway,
}

impl std::fmt::Display for TopologyNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopologyNodeType::Device => write!(f, "device"),
            TopologyNodeType::Interface => write!(f, "interface"),
            TopologyNodeType::Subnet => write!(f, "subnet"),
            TopologyNodeType::Gateway => write!(f, "gateway"),
        }
    }
}

impl std::str::FromStr for TopologyNodeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "device" => Ok(TopologyNodeType::Device),
            "interface" => Ok(TopologyNodeType::Interface),
            "subnet" => Ok(TopologyNodeType::Subnet),
            "gateway" => Ok(TopologyNodeType::Gateway),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TopologyEdgeType {
    Contains,
    ConnectedTo,
    RoutesThrough,
    L2Adjacency,
    GatewayOf,
    DependsOn,
}

impl std::fmt::Display for TopologyEdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopologyEdgeType::Contains => write!(f, "contains"),
            TopologyEdgeType::ConnectedTo => write!(f, "connected_to"),
            TopologyEdgeType::RoutesThrough => write!(f, "routes_through"),
            TopologyEdgeType::L2Adjacency => write!(f, "l2_adjacency"),
            TopologyEdgeType::GatewayOf => write!(f, "gateway_of"),
            TopologyEdgeType::DependsOn => write!(f, "depends_on"),
        }
    }
}

impl std::str::FromStr for TopologyEdgeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contains" => Ok(TopologyEdgeType::Contains),
            "connected_to" => Ok(TopologyEdgeType::ConnectedTo),
            "routes_through" => Ok(TopologyEdgeType::RoutesThrough),
            "l2_adjacency" => Ok(TopologyEdgeType::L2Adjacency),
            "gateway_of" => Ok(TopologyEdgeType::GatewayOf),
            "depends_on" => Ok(TopologyEdgeType::DependsOn),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Arp,
    Ndp,
    Snmp,
    Lldp,
    RoutingTable,
    Manual,
    Discovery,
}

impl std::fmt::Display for Provenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provenance::Arp => write!(f, "arp"),
            Provenance::Ndp => write!(f, "ndp"),
            Provenance::Snmp => write!(f, "snmp"),
            Provenance::Lldp => write!(f, "lldp"),
            Provenance::RoutingTable => write!(f, "routing_table"),
            Provenance::Manual => write!(f, "manual"),
            Provenance::Discovery => write!(f, "discovery"),
        }
    }
}

impl std::str::FromStr for Provenance {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "arp" => Ok(Provenance::Arp),
            "ndp" => Ok(Provenance::Ndp),
            "snmp" => Ok(Provenance::Snmp),
            "lldp" => Ok(Provenance::Lldp),
            "routing_table" => Ok(Provenance::RoutingTable),
            "manual" => Ok(Provenance::Manual),
            "discovery" => Ok(Provenance::Discovery),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopologyNode {
    pub id: String,
    pub node_type: TopologyNodeType,
    pub label: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub state: NodeState,
    pub observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NodeState {
    Online,
    Degraded,
    Offline,
    Unknown,
}

impl std::fmt::Display for NodeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeState::Online => write!(f, "online"),
            NodeState::Degraded => write!(f, "degraded"),
            NodeState::Offline => write!(f, "offline"),
            NodeState::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for NodeState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "online" => Ok(NodeState::Online),
            "degraded" => Ok(NodeState::Degraded),
            "offline" => Ok(NodeState::Offline),
            "unknown" => Ok(NodeState::Unknown),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TopologyEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: TopologyEdgeType,
    pub provenance: Provenance,
    pub confidence: f64,
    pub observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologySnapshot {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyDiff {
    pub added_nodes: Vec<TopologyNode>,
    pub removed_nodes: Vec<TopologyNode>,
    pub added_edges: Vec<TopologyEdge>,
    pub removed_edges: Vec<TopologyEdge>,
    pub changed_nodes: Vec<(TopologyNode, TopologyNode)>,
    pub changed_edges: Vec<(TopologyEdge, TopologyEdge)>,
}

pub struct TopologyGraph {
    graph: DiGraph<TopologyNode, TopologyEdge>,
    node_indices: HashMap<String, NodeIndex>,
}

impl Clone for TopologyGraph {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            node_indices: self.node_indices.clone(),
        }
    }
}

impl TopologyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: TopologyNode) -> NodeIndex {
        let idx = self.graph.add_node(node.clone());
        self.node_indices.insert(node.id, idx);
        idx
    }

    pub fn add_edge(&mut self, edge: TopologyEdge) -> Result<(), anyhow::Error> {
        let source_idx = self
            .node_indices
            .get(&edge.source)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Source node not found: {}", edge.source))?;
        let target_idx = self
            .node_indices
            .get(&edge.target)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Target node not found: {}", edge.target))?;

        self.graph.add_edge(source_idx, target_idx, edge);
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Option<&TopologyNode> {
        self.node_indices
            .get(id)
            .and_then(|idx| self.graph.node_weight(*idx))
    }

    pub fn get_edge(&self, source: &str, target: &str) -> Option<&TopologyEdge> {
        let source_idx = self.node_indices.get(source)?;
        let target_idx = self.node_indices.get(target)?;
        self.graph
            .find_edge(*source_idx, *target_idx)
            .and_then(|e| self.graph.edge_weight(e))
    }

    pub fn nodes(&self) -> Vec<&TopologyNode> {
        self.graph.node_weights().collect()
    }

    pub fn edges(&self) -> Vec<&TopologyEdge> {
        self.graph.edge_weights().collect()
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn to_snapshot(&self) -> TopologySnapshot {
        TopologySnapshot {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            nodes: self.nodes().into_iter().cloned().collect(),
            edges: self.edges().into_iter().cloned().collect(),
        }
    }

    pub fn diff(&self, other: &TopologyGraph) -> TopologyDiff {
        let mut added_nodes = Vec::new();
        let mut removed_nodes = Vec::new();
        let mut changed_nodes = Vec::new();
        let mut added_edges = Vec::new();
        let mut removed_edges = Vec::new();
        let mut changed_edges = Vec::new();

        // Node diff
        for node in self.nodes() {
            if let Some(other_node) = other.get_node(&node.id) {
                if node != other_node {
                    changed_nodes.push((node.clone(), other_node.clone()));
                }
            } else {
                removed_nodes.push(node.clone());
            }
        }

        for node in other.nodes() {
            if !self.node_indices.contains_key(&node.id) {
                added_nodes.push(node.clone());
            }
        }

        // Edge diff
        for edge in self.edges() {
            if let Some(other_edge) = other.get_edge(&edge.source, &edge.target) {
                if edge != other_edge {
                    changed_edges.push((edge.clone(), other_edge.clone()));
                }
            } else {
                removed_edges.push(edge.clone());
            }
        }

        for edge in other.edges() {
            if self
                .graph
                .find_edge(
                    self.node_indices[&edge.source],
                    self.node_indices[&edge.target],
                )
                .is_none()
            {
                added_edges.push(edge.clone());
            }
        }

        TopologyDiff {
            added_nodes,
            removed_nodes,
            added_edges,
            removed_edges,
            changed_nodes,
            changed_edges,
        }
    }
}

impl Default for TopologyGraph {
    fn default() -> Self {
        Self::new()
    }
}
