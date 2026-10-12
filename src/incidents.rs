use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum IncidentError {
    #[error("invalid state transition: {from:?} -> {to:?}")]
    InvalidTransition {
        from: IncidentState,
        to: IncidentState,
    },
    #[error("incident not found: {0}")]
    NotFound(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("date parse error: {0}")]
    DateParse(#[from] chrono::ParseError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IncidentState {
    Open,
    Acknowledged,
    Resolved,
    Closed,
}

impl IncidentState {
    pub fn valid_transitions(&self) -> Vec<IncidentState> {
        match self {
            IncidentState::Open => vec![IncidentState::Acknowledged, IncidentState::Resolved],
            IncidentState::Acknowledged => vec![IncidentState::Resolved, IncidentState::Open],
            IncidentState::Resolved => vec![IncidentState::Closed, IncidentState::Open],
            IncidentState::Closed => vec![],
        }
    }

    pub fn can_transition_to(&self, target: &IncidentState) -> bool {
        self.valid_transitions().contains(target)
    }
}

impl std::str::FromStr for IncidentState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(IncidentState::Open),
            "acknowledged" => Ok(IncidentState::Acknowledged),
            "resolved" => Ok(IncidentState::Resolved),
            "closed" => Ok(IncidentState::Closed),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for IncidentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncidentState::Open => write!(f, "open"),
            IncidentState::Acknowledged => write!(f, "acknowledged"),
            IncidentState::Resolved => write!(f, "resolved"),
            IncidentState::Closed => write!(f, "closed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IncidentSeverity {
    Info,
    Warning,
    Critical,
}

impl IncidentSeverity {
    pub fn to_string(&self) -> &'static str {
        match self {
            IncidentSeverity::Info => "info",
            IncidentSeverity::Warning => "warning",
            IncidentSeverity::Critical => "critical",
        }
    }
}

impl std::str::FromStr for IncidentSeverity {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "info" => Ok(IncidentSeverity::Info),
            "warning" => Ok(IncidentSeverity::Warning),
            "critical" => Ok(IncidentSeverity::Critical),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for IncidentSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentTransition {
    pub from: IncidentState,
    pub to: IncidentState,
    pub timestamp: DateTime<Utc>,
    pub actor: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub dedup_key: String,
    pub correlation_id: Option<String>,
    pub entity_id: String,
    pub entity_type: String,
    pub severity: IncidentSeverity,
    pub state: IncidentState,
    pub title: String,
    pub description: Option<String>,
    pub evidence: Vec<serde_json::Value>,
    pub labels: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<String>,
    pub closed_at: Option<DateTime<Utc>>,
    pub transitions: Vec<IncidentTransition>,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub silence_until: Option<DateTime<Utc>>,
    // Root cause analysis fields (P4)
    pub root_cause_hypothesis_id: Option<String>,
    pub root_cause_confirmed: bool,
    pub root_cause_confirmed_at: Option<DateTime<Utc>>,
    pub root_cause_confirmed_by: Option<String>,
}

impl Incident {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dedup_key: String,
        entity_id: String,
        entity_type: String,
        severity: IncidentSeverity,
        title: String,
        description: Option<String>,
        evidence: Vec<serde_json::Value>,
        labels: HashMap<String, String>,
        correlation_id: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            dedup_key,
            correlation_id,
            entity_id,
            entity_type,
            severity,
            state: IncidentState::Open,
            title,
            description,
            evidence,
            labels,
            created_at: now,
            updated_at: now,
            acknowledged_at: None,
            acknowledged_by: None,
            resolved_at: None,
            resolved_by: None,
            closed_at: None,
            transitions: vec![],
            cooldown_until: None,
            silence_until: None,
            root_cause_hypothesis_id: None,
            root_cause_confirmed: false,
            root_cause_confirmed_at: None,
            root_cause_confirmed_by: None,
        }
    }

    pub fn transition(
        &mut self,
        to: IncidentState,
        actor: Option<String>,
        reason: Option<String>,
    ) -> Result<(), IncidentError> {
        if !self.state.can_transition_to(&to) {
            return Err(IncidentError::InvalidTransition {
                from: self.state.clone(),
                to: to.clone(),
            });
        }

        let transition = IncidentTransition {
            from: self.state.clone(),
            to: to.clone(),
            timestamp: Utc::now(),
            actor,
            reason,
        };

        match to {
            IncidentState::Acknowledged => {
                self.acknowledged_at = Some(Utc::now());
                self.acknowledged_by = transition.actor.clone();
            }
            IncidentState::Resolved => {
                self.resolved_at = Some(Utc::now());
                self.resolved_by = transition.actor.clone();
            }
            IncidentState::Closed => {
                self.closed_at = Some(Utc::now());
            }
            IncidentState::Open => {
                // Re-opening an incident
                self.resolved_at = None;
                self.resolved_by = None;
                self.closed_at = None;
            }
        }

        self.state = to;
        self.updated_at = Utc::now();
        self.transitions.push(transition);

        Ok(())
    }

    pub fn is_in_cooldown(&self) -> bool {
        self.cooldown_until.is_some_and(|until| Utc::now() < until)
    }

    pub fn is_silenced(&self) -> bool {
        self.silence_until.is_some_and(|until| Utc::now() < until)
    }

    pub fn set_cooldown(&mut self, duration_seconds: u64) {
        self.cooldown_until = Some(Utc::now() + chrono::Duration::seconds(duration_seconds as i64));
    }

    pub fn set_silence(&mut self, duration_seconds: u64) {
        self.silence_until = Some(Utc::now() + chrono::Duration::seconds(duration_seconds as i64));
    }

    pub fn acknowledge(
        &mut self,
        actor: String,
        reason: Option<String>,
    ) -> Result<(), IncidentError> {
        self.transition(IncidentState::Acknowledged, Some(actor), reason)
    }

    pub fn resolve(&mut self, actor: String, reason: Option<String>) -> Result<(), IncidentError> {
        self.transition(IncidentState::Resolved, Some(actor), reason)
    }

    pub fn close(&mut self, actor: String, reason: Option<String>) -> Result<(), IncidentError> {
        self.transition(IncidentState::Closed, Some(actor), reason)
    }

    pub fn reopen(&mut self, actor: String, reason: Option<String>) -> Result<(), IncidentError> {
        self.transition(IncidentState::Open, Some(actor), reason)
    }
}

#[derive(sqlx::FromRow)]
struct IncidentRow {
    id: String,
    dedup_key: String,
    correlation_id: Option<String>,
    entity_id: String,
    entity_type: String,
    severity: String,
    state: String,
    title: String,
    description: Option<String>,
    evidence: String,
    labels: String,
    created_at: String,
    updated_at: String,
    acknowledged_at: Option<String>,
    acknowledged_by: Option<String>,
    resolved_at: Option<String>,
    resolved_by: Option<String>,
    closed_at: Option<String>,
    transitions: String,
    cooldown_until: Option<String>,
    silence_until: Option<String>,
    root_cause_hypothesis_id: Option<String>,
    root_cause_confirmed: bool,
    root_cause_confirmed_at: Option<String>,
    root_cause_confirmed_by: Option<String>,
}

impl IncidentRow {
    fn to_incident(&self) -> Result<Incident, IncidentError> {
        let severity = match self.severity.as_str() {
            "info" => IncidentSeverity::Info,
            "warning" => IncidentSeverity::Warning,
            "critical" => IncidentSeverity::Critical,
            _ => IncidentSeverity::Warning,
        };

        let state = match self.state.as_str() {
            "open" => IncidentState::Open,
            "acknowledged" => IncidentState::Acknowledged,
            "resolved" => IncidentState::Resolved,
            "closed" => IncidentState::Closed,
            _ => IncidentState::Open,
        };

        Ok(Incident {
            id: self.id.clone(),
            dedup_key: self.dedup_key.clone(),
            correlation_id: self.correlation_id.clone(),
            entity_id: self.entity_id.clone(),
            entity_type: self.entity_type.clone(),
            severity,
            state,
            title: self.title.clone(),
            description: self.description.clone(),
            evidence: serde_json::from_str(&self.evidence)?,
            labels: serde_json::from_str(&self.labels)?,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
            acknowledged_at: self
                .acknowledged_at
                .as_ref()
                .map(|s| s.parse())
                .transpose()?,
            acknowledged_by: self.acknowledged_by.clone(),
            resolved_at: self.resolved_at.as_ref().map(|s| s.parse()).transpose()?,
            resolved_by: self.resolved_by.clone(),
            closed_at: self.closed_at.as_ref().map(|s| s.parse()).transpose()?,
            transitions: serde_json::from_str(&self.transitions)?,
            cooldown_until: self
                .cooldown_until
                .as_ref()
                .map(|s| s.parse())
                .transpose()?,
            silence_until: self.silence_until.as_ref().map(|s| s.parse()).transpose()?,
            root_cause_hypothesis_id: self.root_cause_hypothesis_id.clone(),
            root_cause_confirmed: self.root_cause_confirmed,
            root_cause_confirmed_at: self
                .root_cause_confirmed_at
                .as_ref()
                .map(|s| s.parse())
                .transpose()?,
            root_cause_confirmed_by: self.root_cause_confirmed_by.clone(),
        })
    }
}

pub struct IncidentStore {
    pool: sqlx::SqlitePool,
}

impl IncidentStore {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, incident: &Incident) -> Result<Incident, IncidentError> {
        let evidence_json = serde_json::to_string(&incident.evidence)?;
        let labels_json = serde_json::to_string(&incident.labels)?;
        let transitions_json = serde_json::to_string(&incident.transitions)?;

        sqlx::query(
            "INSERT INTO incidents (\
                id, dedup_key, correlation_id, entity_id, entity_type, severity, state,\
                title, description, evidence, labels, created_at, updated_at,\
                acknowledged_at, acknowledged_by, resolved_at, resolved_by, closed_at,\
                transitions, cooldown_until, silence_until,\
                root_cause_hypothesis_id, root_cause_confirmed, root_cause_confirmed_at, root_cause_confirmed_by\
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&incident.id)
        .bind(&incident.dedup_key)
        .bind(&incident.correlation_id)
        .bind(&incident.entity_id)
        .bind(&incident.entity_type)
        .bind(incident.severity.to_string())
        .bind(incident.state.to_string())
        .bind(&incident.title)
        .bind(&incident.description)
        .bind(&evidence_json)
        .bind(&labels_json)
        .bind(incident.created_at.to_rfc3339())
        .bind(incident.updated_at.to_rfc3339())
        .bind(incident.acknowledged_at.map(|dt| dt.to_rfc3339()))
        .bind(&incident.acknowledged_by)
        .bind(incident.resolved_at.map(|dt| dt.to_rfc3339()))
        .bind(&incident.resolved_by)
        .bind(incident.closed_at.map(|dt| dt.to_rfc3339()))
        .bind(&transitions_json)
        .bind(incident.cooldown_until.map(|dt| dt.to_rfc3339()))
        .bind(incident.silence_until.map(|dt| dt.to_rfc3339()))
        .bind(&incident.root_cause_hypothesis_id)
        .bind(incident.root_cause_confirmed)
        .bind(incident.root_cause_confirmed_at.map(|dt| dt.to_rfc3339()))
        .bind(&incident.root_cause_confirmed_by)
        .execute(&self.pool)
        .await?;

        Ok(incident.clone())
    }

    pub async fn find_by_dedup_key(
        &self,
        dedup_key: &str,
    ) -> Result<Option<Incident>, IncidentError> {
        let row = sqlx::query_as::<_, IncidentRow>(
            "SELECT * FROM incidents WHERE dedup_key = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(dedup_key)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.to_incident()).transpose()
    }

    pub async fn find_by_id(&self, id: &str) -> Result<Option<Incident>, IncidentError> {
        let row = sqlx::query_as::<_, IncidentRow>("SELECT * FROM incidents WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|r| r.to_incident()).transpose()
    }

    pub async fn update(&self, incident: &Incident) -> Result<(), IncidentError> {
        use sqlx::query;

        let evidence_json = serde_json::to_string(&incident.evidence)?;
        let labels_json = serde_json::to_string(&incident.labels)?;
        let transitions_json = serde_json::to_string(&incident.transitions)?;

        query(
            r#"
            UPDATE incidents SET
                correlation_id = ?, entity_id = ?, entity_type = ?, severity = ?, state = ?,
                title = ?, description = ?, evidence = ?, labels = ?, updated_at = ?,
                acknowledged_at = ?, acknowledged_by = ?, resolved_at = ?, resolved_by = ?,
                closed_at = ?, transitions = ?, cooldown_until = ?, silence_until = ?,
                root_cause_hypothesis_id = ?, root_cause_confirmed = ?, root_cause_confirmed_at = ?, root_cause_confirmed_by = ?
            WHERE id = ?
            "#,
        )
        .bind(&incident.correlation_id)
        .bind(&incident.entity_id)
        .bind(&incident.entity_type)
        .bind(incident.severity.to_string())
        .bind(incident.state.to_string())
        .bind(&incident.title)
        .bind(&incident.description)
        .bind(&evidence_json)
        .bind(&labels_json)
        .bind(incident.updated_at.to_rfc3339())
        .bind(incident.acknowledged_at.map(|dt| dt.to_rfc3339()))
        .bind(&incident.acknowledged_by)
        .bind(incident.resolved_at.map(|dt| dt.to_rfc3339()))
        .bind(&incident.resolved_by)
        .bind(incident.closed_at.map(|dt| dt.to_rfc3339()))
        .bind(&transitions_json)
        .bind(incident.cooldown_until.map(|dt| dt.to_rfc3339()))
        .bind(incident.silence_until.map(|dt| dt.to_rfc3339()))
        .bind(&incident.root_cause_hypothesis_id)
        .bind(incident.root_cause_confirmed)
        .bind(incident.root_cause_confirmed_at.map(|dt| dt.to_rfc3339()))
        .bind(&incident.root_cause_confirmed_by)
        .bind(&incident.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_open(&self) -> Result<Vec<Incident>, IncidentError> {
        let rows = sqlx::query_as::<_, IncidentRow>(
            "SELECT * FROM incidents WHERE state IN ('open', 'acknowledged', 'resolved') ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.to_incident()).collect()
    }

    pub async fn find_by_hypothesis_id(
        &self,
        hypothesis_id: &str,
    ) -> Result<Option<Incident>, IncidentError> {
        let row = sqlx::query_as::<_, IncidentRow>(
            "SELECT * FROM incidents WHERE root_cause_hypothesis_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(hypothesis_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| r.to_incident()).transpose()
    }
}

/// Confirm or reject a root cause hypothesis for an incident
pub async fn confirm_root_cause(
    store: &IncidentStore,
    incident_id: &str,
    hypothesis_id: &str,
    confirmed: bool,
    actor: String,
) -> Result<(), IncidentError> {
    let mut incident = store
        .find_by_id(incident_id)
        .await?
        .ok_or_else(|| IncidentError::NotFound(incident_id.to_string()))?;

    incident.root_cause_hypothesis_id = Some(hypothesis_id.to_string());
    incident.root_cause_confirmed = confirmed;
    incident.root_cause_confirmed_at = Some(Utc::now());
    incident.root_cause_confirmed_by = Some(actor);

    store.update(&incident).await?;
    Ok(())
}

/// Incident Engine - consumes events from EventBus and manages incident lifecycle
pub struct IncidentEngine {
    pub store: IncidentStore,
    event_bus: broadcast::Receiver<crate::events::Event>,
    cooldown_seconds: u64,
}

impl IncidentEngine {
    pub fn new(
        store: IncidentStore,
        event_bus: broadcast::Receiver<crate::events::Event>,
        cooldown_seconds: u64,
    ) -> Self {
        Self {
            store,
            event_bus,
            cooldown_seconds,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        while let Ok(event) = self.event_bus.recv().await {
            if let Err(e) = self.process_event(event).await {
                tracing::error!("Incident engine error processing event: {}", e);
            }
        }
        Ok(())
    }

    pub async fn process_event(&mut self, event: crate::events::Event) -> Result<()> {
        use crate::events::EventType;

        match event.event_type {
            EventType::CheckFailed | EventType::ThresholdBreached | EventType::AnomalyDetected => {
                self.handle_firing_event(event).await
            }
            EventType::CheckCompleted | EventType::ThresholdRecovered => {
                self.handle_recovery_event(event).await
            }
            _ => Ok(()),
        }
    }

    async fn handle_firing_event(&mut self, event: crate::events::Event) -> Result<()> {
        use crate::events::{EventType, Severity};

        // Check cooldown
        if let Some(mut existing) = self.store.find_by_dedup_key(&event.dedup_key).await? {
            if existing.is_in_cooldown() {
                return Ok(());
            }
            if existing.is_silenced() {
                return Ok(());
            }
            // Update existing incident with new evidence
            existing
                .evidence
                .push(serde_json::to_value(&event.evidence)?);
            existing.updated_at = Utc::now();
            existing.set_cooldown(self.cooldown_seconds);
            self.store.update(&existing).await?;
            return Ok(());
        }

        // Determine severity from event
        let severity = match event.severity {
            Severity::Info => IncidentSeverity::Info,
            Severity::Warning => IncidentSeverity::Warning,
            Severity::Critical => IncidentSeverity::Critical,
        };

        // Create new incident
        let title = match event.event_type {
            EventType::CheckFailed => format!("Check failed: {}", event.entity_id),
            EventType::ThresholdBreached => {
                if let Some(th) = &event.evidence.threshold {
                    format!(
                        "Threshold breached: {} {} {}",
                        th.metric, th.operator, th.threshold
                    )
                } else {
                    format!("Threshold breached: {}", event.entity_id)
                }
            }
            EventType::AnomalyDetected => {
                if let Some(anomaly) = &event.evidence.anomaly {
                    format!(
                        "Anomaly detected: {} {} on {} ({})",
                        anomaly.anomaly_type, anomaly.metric, anomaly.entity_id, anomaly.protocol
                    )
                } else {
                    format!("Anomaly detected: {}", event.entity_id)
                }
            }
            _ => format!("Event: {}", event.entity_id),
        };

        let mut incident = Incident::new(
            event.dedup_key.clone(),
            event.entity_id.clone(),
            format!("{:?}", event.entity_type),
            severity,
            title,
            Some(format!("{:?}", event.event_type)),
            vec![serde_json::to_value(&event.evidence)?],
            HashMap::new(),
            event.correlation_id.clone(),
        );
        incident.set_cooldown(self.cooldown_seconds);

        self.store.create(&incident).await?;
        Ok(())
    }

    async fn handle_recovery_event(&mut self, event: crate::events::Event) -> Result<()> {
        if let Some(mut existing) = self.store.find_by_dedup_key(&event.dedup_key).await? {
            if existing.state == IncidentState::Open
                || existing.state == IncidentState::Acknowledged
            {
                existing.resolve("auto".to_string(), Some("Condition recovered".to_string()))?;
                self.store.update(&existing).await?;
            }
        }
        Ok(())
    }
}
