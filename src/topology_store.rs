use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};

use crate::topology::{TopologyEdge, TopologyNode, TopologySnapshot};

#[derive(FromRow)]
struct NodeRow {
    id: String,
    node_type: String,
    label: String,
    properties: String,
    state: String,
    observed_at: String,
    created_at: String,
    updated_at: String,
}

#[derive(FromRow)]
struct EdgeRow {
    id: String,
    source: String,
    target: String,
    edge_type: String,
    provenance: String,
    confidence: f64,
    observed_at: String,
    created_at: String,
    updated_at: String,
}

#[derive(FromRow)]
struct SnapshotRow {
    id: String,
    timestamp: String,
    nodes_json: String,
    edges_json: String,
}

pub struct TopologyStore {
    pool: SqlitePool,
}

impl TopologyStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_node(&self, node: &TopologyNode) -> Result<()> {
        use sqlx::query;

        let properties_json = serde_json::to_string(&node.properties)?;

        query(
            r#"
            INSERT INTO topology_nodes (id, node_type, label, properties, state, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                node_type = excluded.node_type,
                label = excluded.label,
                properties = excluded.properties,
                state = excluded.state,
                observed_at = excluded.observed_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&node.id)
        .bind(node.node_type.to_string())
        .bind(&node.label)
        .bind(&properties_json)
        .bind(node.state.to_string())
        .bind(node.observed_at.to_rfc3339())
        .bind(node.created_at.to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_edge(&self, edge: &TopologyEdge) -> Result<()> {
        use sqlx::query;

        query(
            r#"
            INSERT INTO topology_edges (id, source, target, edge_type, provenance, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                source = excluded.source,
                target = excluded.target,
                edge_type = excluded.edge_type,
                provenance = excluded.provenance,
                confidence = excluded.confidence,
                observed_at = excluded.observed_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&edge.id)
        .bind(&edge.source)
        .bind(&edge.target)
        .bind(edge.edge_type.to_string())
        .bind(edge.provenance.to_string())
        .bind(edge.confidence)
        .bind(edge.observed_at.to_rfc3339())
        .bind(edge.created_at.to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<TopologyNode>> {
        use sqlx::query_as;

        let row = query_as::<_, NodeRow>("SELECT * FROM topology_nodes WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|row| self.row_to_node(row)).transpose()
    }

    pub async fn get_edge(&self, source: &str, target: &str) -> Result<Option<TopologyEdge>> {
        use sqlx::query_as;

        let row =
            query_as::<_, EdgeRow>("SELECT * FROM topology_edges WHERE source = ? AND target = ?")
                .bind(source)
                .bind(target)
                .fetch_optional(&self.pool)
                .await?;

        row.map(|row| self.row_to_edge(row)).transpose()
    }

    pub async fn list_nodes(&self) -> Result<Vec<TopologyNode>> {
        use sqlx::query_as;

        let rows = query_as::<_, NodeRow>("SELECT * FROM topology_nodes")
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(|row| self.row_to_node(row)).collect()
    }

    pub async fn list_edges(&self) -> Result<Vec<TopologyEdge>> {
        use sqlx::query_as;

        let rows = query_as::<_, EdgeRow>("SELECT * FROM topology_edges")
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(|row| self.row_to_edge(row)).collect()
    }

    pub async fn create_snapshot(&self, snapshot: &TopologySnapshot) -> Result<()> {
        use sqlx::query;

        let nodes_json = serde_json::to_string(&snapshot.nodes)?;
        let edges_json = serde_json::to_string(&snapshot.edges)?;

        query(
            r#"
            INSERT INTO topology_snapshots (id, timestamp, nodes_json, edges_json)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(snapshot.id.clone())
        .bind(snapshot.timestamp.to_rfc3339())
        .bind(&nodes_json)
        .bind(&edges_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_latest_snapshot(&self) -> Result<Option<TopologySnapshot>> {
        use sqlx::query_as;

        let row = query_as::<_, SnapshotRow>(
            "SELECT * FROM topology_snapshots ORDER BY timestamp DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            let nodes = serde_json::from_str(&row.nodes_json)?;
            let edges = serde_json::from_str(&row.edges_json)?;
            Ok(TopologySnapshot {
                id: row.id,
                timestamp: row.timestamp.parse()?,
                nodes,
                edges,
            })
        })
        .transpose()
    }

    pub async fn get_snapshots_since(&self, since: DateTime<Utc>) -> Result<Vec<TopologySnapshot>> {
        use sqlx::query_as;

        let rows = query_as::<_, SnapshotRow>(
            "SELECT * FROM topology_snapshots WHERE timestamp >= ? ORDER BY timestamp ASC",
        )
        .bind(since.to_rfc3339())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let nodes = serde_json::from_str(&row.nodes_json)?;
                let edges = serde_json::from_str(&row.edges_json)?;
                Ok(TopologySnapshot {
                    id: row.id,
                    timestamp: row.timestamp.parse()?,
                    nodes,
                    edges,
                })
            })
            .collect()
    }

    fn row_to_node(&self, row: NodeRow) -> Result<TopologyNode> {
        use crate::topology::{NodeState, TopologyNodeType};

        let node_type = match row.node_type.as_str() {
            "device" => TopologyNodeType::Device,
            "interface" => TopologyNodeType::Interface,
            "subnet" => TopologyNodeType::Subnet,
            "gateway" => TopologyNodeType::Gateway,
            _ => TopologyNodeType::Device,
        };

        let state = match row.state.as_str() {
            "online" => NodeState::Online,
            "degraded" => NodeState::Degraded,
            "offline" => NodeState::Offline,
            _ => NodeState::Unknown,
        };

        Ok(TopologyNode {
            id: row.id,
            node_type,
            label: row.label,
            properties: serde_json::from_str(&row.properties)?,
            state,
            observed_at: row.observed_at.parse()?,
            created_at: row.created_at.parse()?,
            updated_at: row.updated_at.parse()?,
        })
    }

    fn row_to_edge(&self, row: EdgeRow) -> Result<TopologyEdge> {
        use crate::topology::{Provenance, TopologyEdgeType};

        let edge_type = match row.edge_type.as_str() {
            "contains" => TopologyEdgeType::Contains,
            "connected_to" => TopologyEdgeType::ConnectedTo,
            "routes_through" => TopologyEdgeType::RoutesThrough,
            "l2_adjacency" => TopologyEdgeType::L2Adjacency,
            "gateway_of" => TopologyEdgeType::GatewayOf,
            "depends_on" => TopologyEdgeType::DependsOn,
            _ => TopologyEdgeType::ConnectedTo,
        };

        let provenance = match row.provenance.as_str() {
            "arp" => Provenance::Arp,
            "ndp" => Provenance::Ndp,
            "snmp" => Provenance::Snmp,
            "lldp" => Provenance::Lldp,
            "routing_table" => Provenance::RoutingTable,
            "manual" => Provenance::Manual,
            "discovery" => Provenance::Discovery,
            _ => Provenance::Discovery,
        };

        Ok(TopologyEdge {
            id: row.id,
            source: row.source,
            target: row.target,
            edge_type,
            provenance,
            confidence: row.confidence,
            observed_at: row.observed_at.parse()?,
            created_at: row.created_at.parse()?,
            updated_at: row.updated_at.parse()?,
        })
    }
}
