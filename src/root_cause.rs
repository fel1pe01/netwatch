use anyhow::Result;
use chrono::{DateTime, Utc};
use petgraph::algo::dijkstra;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::topology::{TopologyEdge, TopologyGraph, TopologyNode, TopologyNodeType};
use crate::topology_store::TopologyStore;

/// Root cause hypothesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hypothesis {
    pub id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub entity_label: String,
    pub confidence: f64,
    pub evidence: Vec<HypothesisEvidence>,
    pub dependency_path: Vec<DependencyLink>,
    pub time_window_start: DateTime<Utc>,
    pub time_window_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub confirmed: bool,
    pub confirmed_by: Option<String>,
    pub confirmed_at: Option<DateTime<Utc>>,
}

/// Evidence supporting a hypothesis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HypothesisEvidence {
    pub check_id: String,
    pub entity_id: String,
    pub protocol: String,
    pub success: bool,
    pub latency_ms: Option<u128>,
    pub timestamp: DateTime<Utc>,
    pub weight: f64,
}

/// Link in the dependency chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyLink {
    pub from_entity_id: String,
    pub from_entity_label: String,
    pub to_entity_id: String,
    pub to_entity_label: String,
    pub relationship: String,
    pub provenance: String,
    pub confidence: f64,
}

/// Root cause analyzer
pub struct RootCauseAnalyzer {
    topology_store: Arc<TopologyStore>,
    storage: Arc<crate::storage::Storage>,
}

impl RootCauseAnalyzer {
    pub fn new(topology_store: Arc<TopologyStore>, storage: Arc<crate::storage::Storage>) -> Self {
        Self {
            topology_store,
            storage,
        }
    }

    /// Analyze failing checks and generate root cause hypotheses
    pub async fn analyze(
        &self,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<Hypothesis>> {
        let until = until.unwrap_or_else(Utc::now);

        // Get failing observations in the time window
        let failing_observations = self.get_failing_observations(since, until).await?;

        if failing_observations.is_empty() {
            return Ok(vec![]);
        }

        // Group failing observations by entity
        let mut entity_failures: HashMap<String, Vec<FailingObservation>> = HashMap::new();
        for obs in failing_observations {
            entity_failures
                .entry(obs.device_id.clone())
                .or_default()
                .push(obs);
        }

        // Build topology graph for dependency analysis
        let graph = self.build_dependency_graph().await?;

        // Generate hypotheses
        let mut hypotheses = Vec::new();

        // For each failing entity, find potential root causes
        for (entity_id, failures) in entity_failures {
            if let Some(hypothesis) = self
                .generate_hypothesis(&entity_id, &failures, &graph)
                .await?
            {
                hypotheses.push(hypothesis);
            }
        }

        // Sort hypotheses by confidence (descending)
        hypotheses.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        Ok(hypotheses)
    }

    /// Get failing observations in a time window
    async fn get_failing_observations(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<FailingObservation>> {
        // Query observations table for failed checks
        let rows = sqlx::query(
            r#"
            SELECT id, device_id, metric, value, source, confidence, observed_at, created_at, updated_at
            FROM observations
            WHERE observed_at >= ? AND observed_at <= ? AND value = 0
            ORDER BY observed_at DESC
            "#,
        )
        .bind(since.to_rfc3339())
        .bind(until.to_rfc3339())
        .fetch_all(&self.storage.pool)
        .await?;

        let mut observations = Vec::new();
        for row in rows {
            let device_id: String = row.get("device_id");
            let metric: String = row.get("metric");
            let value: f64 = row.get("value");
            let observed_at: String = row.get("observed_at");

            observations.push(FailingObservation {
                device_id,
                metric,
                value,
                observed_at: observed_at.parse()?,
            });
        }

        Ok(observations)
    }

    /// Build dependency graph from topology
    async fn build_dependency_graph(&self) -> Result<DependencyGraph> {
        let nodes = self.topology_store.list_nodes().await?;
        let edges = self.topology_store.list_edges().await?;

        let mut graph = DependencyGraph::new();

        // Add nodes
        for node in nodes {
            graph.add_node(node);
        }

        // Add edges (only DependsOn and GatewayOf relationships for root cause)
        for edge in edges {
            if matches!(
                edge.edge_type,
                crate::topology::TopologyEdgeType::DependsOn
                    | crate::topology::TopologyEdgeType::GatewayOf
            ) {
                graph.add_edge(edge);
            }
        }

        Ok(graph)
    }

    /// Build full topology graph for path finding
    #[allow(dead_code)]
    async fn build_topology_graph(&self) -> Result<TopologyGraph> {
        let nodes = self.topology_store.list_nodes().await?;
        let edges = self.topology_store.list_edges().await?;

        let mut graph = TopologyGraph::new();

        for node in nodes {
            graph.add_node(node);
        }
        for edge in edges {
            graph.add_edge(edge)?;
        }

        Ok(graph)
    }

    /// Generate hypothesis for a failing entity
    async fn generate_hypothesis(
        &self,
        entity_id: &str,
        failures: &[FailingObservation],
        graph: &DependencyGraph,
    ) -> Result<Option<Hypothesis>> {
        // Find all entities that depend on this entity (upstream in dependency graph)
        let upstream_causes = graph.find_upstream_causes(entity_id);

        if upstream_causes.is_empty() {
            // No upstream dependencies - this entity might be the root cause
            return Ok(Some(
                self.create_hypothesis(entity_id, failures, vec![], true)
                    .await?,
            ));
        }

        // Score each potential cause
        let mut scored_causes = Vec::new();
        for cause in upstream_causes {
            let score = self.score_cause(&cause, failures).await?;
            if score > 0.0 {
                scored_causes.push((cause, score));
            }
        }

        // Sort by score descending
        scored_causes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if let Some((best_cause, confidence)) = scored_causes.first() {
            let dependency_path = self
                .build_dependency_path(entity_id, &best_cause.id)
                .await?;
            return Ok(Some(
                self.create_hypothesis(
                    &best_cause.id,
                    failures,
                    dependency_path,
                    *confidence > 0.5, // treat as root if confidence > 0.5
                )
                .await?,
            ));
        }

        // Fallback: no clear upstream cause found
        Ok(Some(
            self.create_hypothesis(entity_id, failures, vec![], false)
                .await?,
        ))
    }

    /// Score a potential cause based on evidence
    async fn score_cause(
        &self,
        cause: &TopologyNode,
        failures: &[FailingObservation],
    ) -> Result<f64> {
        let mut score = 0.5; // Base score

        // Check if the cause entity itself has failures
        let cause_failures: Vec<_> = failures
            .iter()
            .filter(|f| f.device_id == cause.id)
            .collect();
        if !cause_failures.is_empty() {
            score += 0.3; // Direct evidence
        }

        // Check if the cause is a gateway or critical infrastructure
        if matches!(cause.node_type, TopologyNodeType::Gateway) {
            score += 0.2;
        }

        // Check temporal correlation (failures around same time)
        let now = Utc::now();
        let recent_failures = cause_failures
            .iter()
            .filter(|f| (now - f.observed_at).num_minutes() < 60)
            .count();

        if recent_failures > 0 {
            score += 0.1 * (recent_failures as f64).min(3.0);
        }

        Ok(score.min(1.0))
    }

    /// Build dependency path from effect to cause
    async fn build_dependency_path(&self, from: &str, to: &str) -> Result<Vec<DependencyLink>> {
        let graph = self.build_dependency_graph().await?;
        let path = graph.find_shortest_path(from, to);

        let mut links = Vec::new();
        for i in 0..path.len().saturating_sub(1) {
            let from_id = &path[i];
            let to_id = &path[i + 1];

            // Find the edge between these nodes
            if let Some(edge) = self.find_edge_between(from_id, to_id).await? {
                links.push(DependencyLink {
                    from_entity_id: from_id.clone(),
                    from_entity_label: edge.source.clone(),
                    to_entity_id: to_id.clone(),
                    to_entity_label: edge.target.clone(),
                    relationship: format!("{:?}", edge.edge_type),
                    provenance: format!("{:?}", edge.provenance),
                    confidence: edge.confidence,
                });
            }
        }

        Ok(links)
    }

    async fn find_edge_between(&self, from: &str, to: &str) -> Result<Option<TopologyEdge>> {
        let edges = self.topology_store.list_edges().await?;
        Ok(edges
            .into_iter()
            .find(|e| e.source == from && e.target == to))
    }

    async fn create_hypothesis(
        &self,
        entity_id: &str,
        failures: &[FailingObservation],
        dependency_path: Vec<DependencyLink>,
        is_root: bool,
    ) -> Result<Hypothesis> {
        let entity = self
            .topology_store
            .get_node(entity_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Entity not found"))?;

        let confidence = if is_root { 0.9 } else { 0.7 };

        let evidence: Vec<HypothesisEvidence> = failures
            .iter()
            .map(|f| HypothesisEvidence {
                check_id: format!("{}-{}", f.device_id, f.metric),
                entity_id: f.device_id.clone(),
                protocol: f.metric.split('_').next().unwrap_or("unknown").to_string(),
                success: false,
                latency_ms: if f.value > 0.0 {
                    Some(f.value as u128)
                } else {
                    None
                },
                timestamp: f.observed_at,
                weight: 1.0,
            })
            .collect();

        Ok(Hypothesis {
            id: Uuid::new_v4().to_string(),
            entity_id: entity_id.to_string(),
            entity_type: format!("{:?}", entity.node_type),
            entity_label: entity.label.clone(),
            confidence,
            evidence,
            dependency_path,
            time_window_start: failures
                .iter()
                .map(|f| f.observed_at)
                .min()
                .unwrap_or(Utc::now()),
            time_window_end: failures
                .iter()
                .map(|f| f.observed_at)
                .max()
                .unwrap_or(Utc::now()),
            created_at: Utc::now(),
            confirmed: false,
            confirmed_by: None,
            confirmed_at: None,
        })
    }
}

/// Failing observation for root cause analysis
#[derive(Debug, Clone)]
struct FailingObservation {
    device_id: String,
    metric: String,
    value: f64,
    observed_at: DateTime<Utc>,
}

/// Dependency graph for root cause analysis
#[derive(Debug)]
struct DependencyGraph {
    graph: DiGraph<TopologyNode, TopologyEdge>,
    node_indices: HashMap<String, NodeIndex>,
}

impl DependencyGraph {
    fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_indices: HashMap::new(),
        }
    }

    fn add_node(&mut self, node: TopologyNode) {
        let idx = self.graph.add_node(node.clone());
        self.node_indices.insert(node.id, idx);
    }

    fn add_edge(&mut self, edge: TopologyEdge) {
        if let (Some(&from_idx), Some(&to_idx)) = (
            self.node_indices.get(&edge.source),
            self.node_indices.get(&edge.target),
        ) {
            self.graph.add_edge(from_idx, to_idx, edge);
        }
    }

    /// Find upstream causes (entities that this entity depends on)
    fn find_upstream_causes(&self, entity_id: &str) -> Vec<TopologyNode> {
        let mut causes = Vec::new();
        if let Some(&idx) = self.node_indices.get(entity_id) {
            // Walk upstream (reverse edges) to find dependencies
            for edge in self
                .graph
                .edges_directed(idx, petgraph::Direction::Incoming)
            {
                let target_idx = edge.source();
                if let Some(node) = self.graph.node_weight(target_idx) {
                    if matches!(
                        node.node_type,
                        TopologyNodeType::Gateway | TopologyNodeType::Device
                    ) {
                        causes.push(node.clone());
                    }
                }
            }
        }
        causes
    }

    /// Find shortest path between two nodes
    fn find_shortest_path(&self, from: &str, to: &str) -> Vec<String> {
        let from_idx = self.node_indices.get(from);
        let to_idx = self.node_indices.get(to);

        if let (Some(&from_idx), Some(&to_idx)) = (from_idx, to_idx) {
            let result = dijkstra(&self.graph, from_idx, Some(to_idx), |e| {
                1.0 / e.weight().confidence
            });
            if result.contains_key(&to_idx) {
                // Reconstruct path
                let mut path = Vec::new();
                let mut current = to_idx;
                path.push(self.graph.node_weight(current).unwrap().id.clone());

                while current != from_idx {
                    for edge in self
                        .graph
                        .edges_directed(current, petgraph::Direction::Incoming)
                    {
                        if result.get(&edge.source())
                            == Some(&(result[&current] - 1.0 / result[&current]))
                        {
                            current = edge.source();
                            path.push(self.graph.node_weight(current).unwrap().id.clone());
                            break;
                        }
                    }
                }
                path.reverse();
            }
        }
        Vec::new()
    }
}
