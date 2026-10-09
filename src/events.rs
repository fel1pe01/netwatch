use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast::{channel, Receiver, Sender};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("broadcast error: {0}")]
    Broadcast(#[from] Box<tokio::sync::broadcast::error::SendError<Event>>),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub schema_version: u32,
    pub event_type: EventType,
    pub event_ts: DateTime<Utc>,
    pub observed_ts: DateTime<Utc>,
    pub entity_id: String,
    pub entity_type: EntityType,
    pub severity: Severity,
    pub state: EventState,
    pub source: String,
    pub evidence: EventEvidence,
    pub dedup_key: String,
    pub correlation_id: Option<String>,
    pub trace_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    CheckCompleted,
    CheckFailed,
    ThresholdBreached,
    ThresholdRecovered,
    DeviceDiscovered,
    DeviceLost,
    InterfaceChanged,
    RouteChanged,
    TopologyChanged,
    IncidentOpened,
    IncidentAcknowledged,
    IncidentResolved,
    IncidentClosed,
    CollectorError,
    CollectorRecovered,
    SchedulerError,
    ConfigChanged,
    MaintenanceWindowStart,
    MaintenanceWindowEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Device,
    Interface,
    Service,
    Subnet,
    Gateway,
    Collector,
    Scheduler,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventState {
    Pending,
    Firing,
    Acknowledged,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEvidence {
    pub check_result: Option<CheckEvidence>,
    pub threshold: Option<ThresholdEvidence>,
    pub topology_diff: Option<TopologyDiffEvidence>,
    pub collector_error: Option<CollectorErrorEvidence>,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckEvidence {
    pub target: String,
    pub protocol: String,
    pub success: bool,
    pub latency_ms: Option<u128>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdEvidence {
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
    pub operator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyDiffEvidence {
    pub added_nodes: Vec<String>,
    pub removed_nodes: Vec<String>,
    pub added_edges: Vec<String>,
    pub removed_edges: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorErrorEvidence {
    pub collector_name: String,
    pub error: String,
    pub target: Option<String>,
}

impl Event {
    pub fn new(
        event_type: EventType,
        entity_id: String,
        entity_type: EntityType,
        severity: Severity,
        source: String,
        evidence: EventEvidence,
    ) -> Self {
        let now = Utc::now();
        let dedup_key = format!("{}:{}", event_type.as_str(), entity_id);
        Self {
            id: Uuid::new_v4().to_string(),
            schema_version: 1,
            event_type,
            event_ts: now,
            observed_ts: now,
            entity_id,
            entity_type,
            severity,
            state: EventState::Pending,
            source,
            evidence,
            dedup_key,
            correlation_id: None,
            trace_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_trace_id(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

impl EventType {
    fn as_str(&self) -> &'static str {
        match self {
            EventType::CheckCompleted => "check_completed",
            EventType::CheckFailed => "check_failed",
            EventType::ThresholdBreached => "threshold_breached",
            EventType::ThresholdRecovered => "threshold_recovered",
            EventType::DeviceDiscovered => "device_discovered",
            EventType::DeviceLost => "device_lost",
            EventType::InterfaceChanged => "interface_changed",
            EventType::RouteChanged => "route_changed",
            EventType::TopologyChanged => "topology_changed",
            EventType::IncidentOpened => "incident_opened",
            EventType::IncidentAcknowledged => "incident_acknowledged",
            EventType::IncidentResolved => "incident_resolved",
            EventType::IncidentClosed => "incident_closed",
            EventType::CollectorError => "collector_error",
            EventType::CollectorRecovered => "collector_recovered",
            EventType::SchedulerError => "scheduler_error",
            EventType::ConfigChanged => "config_changed",
            EventType::MaintenanceWindowStart => "maintenance_window_start",
            EventType::MaintenanceWindowEnd => "maintenance_window_end",
        }
    }
}

pub struct EventBus {
    sender: Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _receiver) = channel(1000);
        Self { sender }
    }

    pub fn subscribe(&self) -> Receiver<Event> {
        self.sender.subscribe()
    }

    pub async fn emit(&self, event: Event) -> Result<(), EventError> {
        self.sender
            .send(event)
            .map(|_| ())
            .map_err(|e| EventError::Broadcast(Box::new(e)))
    }

    pub async fn emit_check_completed(
        &self,
        entity_id: String,
        entity_type: EntityType,
        check_result: CheckEvidence,
    ) -> Result<(), EventError> {
        let event_type = if check_result.success {
            EventType::CheckCompleted
        } else {
            EventType::CheckFailed
        };
        let severity = if check_result.success {
            Severity::Info
        } else {
            Severity::Warning
        };
        let event = Event::new(
            event_type,
            entity_id,
            entity_type,
            severity,
            "monitoring".to_string(),
            EventEvidence {
                check_result: Some(check_result),
                threshold: None,
                topology_diff: None,
                collector_error: None,
                raw: None,
            },
        );
        self.emit(event).await
    }

    pub async fn emit_threshold_breached(
        &self,
        entity_id: String,
        entity_type: EntityType,
        threshold_evidence: ThresholdEvidence,
    ) -> Result<(), EventError> {
        let event = Event::new(
            EventType::ThresholdBreached,
            entity_id,
            entity_type,
            Severity::Critical,
            "monitoring".to_string(),
            EventEvidence {
                check_result: None,
                threshold: Some(threshold_evidence),
                topology_diff: None,
                collector_error: None,
                raw: None,
            },
        );
        self.emit(event).await
    }

    pub async fn emit_collector_error(
        &self,
        collector_name: String,
        error: String,
        target: Option<String>,
    ) -> Result<(), EventError> {
        let event = Event::new(
            EventType::CollectorError,
            collector_name.clone(),
            EntityType::Collector,
            Severity::Critical,
            "collector".to_string(),
            EventEvidence {
                check_result: None,
                threshold: None,
                topology_diff: None,
                collector_error: Some(CollectorErrorEvidence {
                    collector_name,
                    error,
                    target,
                }),
                raw: None,
            },
        );
        self.emit(event).await
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
