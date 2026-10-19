use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{AlertCondition, AlertConfig as ConfigAlertConfig, AlertRule, AlertSeverity};
use crate::events::{CheckEvidence, EntityType, EventBus, EventType, Severity};
use crate::storage::Storage;

/// Alert evaluation result
#[derive(Debug, Clone)]
pub struct AlertEvaluation {
    pub rule_id: String,
    pub rule_name: String,
    pub entity_id: String,
    pub entity_type: String,
    pub protocol: String,
    pub fired: bool,
    pub severity: AlertSeverity,
    pub message: String,
    pub evidence: serde_json::Value,
    pub evaluated_at: DateTime<Utc>,
}

/// Alert rule engine
pub struct AlertEngine {
    storage: Arc<Storage>,
    event_bus: Arc<EventBus>,
    rules: Vec<AlertRule>,
    /// Cache of last evaluation time per rule/entity for cooldown
    last_evaluation: HashMap<String, DateTime<Utc>>,
    /// Cache of active alerts for deduplication
    active_alerts: HashMap<String, AlertEvaluation>,
}

impl AlertEngine {
    pub fn new(storage: Arc<Storage>, event_bus: Arc<EventBus>, config: AlertEngineConfig) -> Self {
        Self {
            storage,
            event_bus,
            rules: config.rules,
            last_evaluation: HashMap::new(),
            active_alerts: HashMap::new(),
        }
    }

    /// Add a new alert rule
    pub fn add_rule(&mut self, rule: AlertRule) {
        self.rules.push(rule);
    }

    /// Remove a rule by ID
    pub fn remove_rule(&mut self, rule_id: &str) {
        self.rules.retain(|r| r.id != rule_id);
    }

    /// Enable/disable a rule
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = enabled;
        }
    }

    /// Evaluate all rules for a specific entity/protocol combination
    pub async fn evaluate_entity(
        &mut self,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
    ) -> Result<Vec<AlertEvaluation>> {
        let mut evaluations = Vec::new();

        // Collect rule indices to evaluate to avoid borrowing issues
        let rule_indices: Vec<usize> = self
            .rules
            .iter()
            .enumerate()
            .filter_map(|(i, rule)| {
                if !rule.enabled {
                    return None;
                }
                if rule.entity_type != entity_type {
                    return None;
                }
                if let Some(ref proto) = rule.protocol {
                    if proto != protocol {
                        return None;
                    }
                }
                let cooldown_key = format!("{}:{}:{}", rule.id, entity_id, protocol);
                if let Some(&last_eval) = self.last_evaluation.get(&cooldown_key) {
                    let cooldown = Duration::seconds(rule.cooldown_seconds as i64);
                    if Utc::now() - last_eval < cooldown {
                        return None;
                    }
                }
                Some(i)
            })
            .collect();

        for idx in rule_indices {
            let rule_id = self.rules[idx].id.clone();
            let cooldown_key = format!("{}:{}:{}", rule_id, entity_id, protocol);

            match self
                .evaluate_rule_by_index(idx, entity_id, entity_type, protocol)
                .await
            {
                Ok(Some(eval)) => {
                    self.last_evaluation.insert(cooldown_key, eval.evaluated_at);
                    evaluations.push(eval);
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!("Error evaluating rule {}: {}", rule_id, e);
                }
            }
        }

        Ok(evaluations)
    }

    /// Evaluate a single rule against an entity by index
    async fn evaluate_rule_by_index(
        &mut self,
        idx: usize,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
    ) -> Result<Option<AlertEvaluation>> {
        // Clone the rule to avoid borrowing issues
        let rule = self.rules[idx].clone();
        self.evaluate_rule(rule, entity_id, entity_type, protocol)
            .await
    }

    /// Evaluate a single rule against an entity
    async fn evaluate_rule(
        &mut self,
        rule: AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let alert_key = format!("{}:{}:{}", rule.id, entity_id, protocol);

        match &rule.condition {
            AlertCondition::Down => {
                self.check_down(&rule, entity_id, entity_type, protocol, &alert_key)
                    .await
            }
            AlertCondition::Up => {
                self.check_up(&rule, entity_id, entity_type, protocol, &alert_key)
                    .await
            }
            AlertCondition::LatencyThreshold { threshold_ms } => {
                self.check_latency_threshold(
                    &rule,
                    entity_id,
                    entity_type,
                    protocol,
                    *threshold_ms,
                    &alert_key,
                )
                .await
            }
            AlertCondition::LatencyPercentile {
                percentile,
                threshold_ms,
                window_seconds,
            } => {
                self.check_latency_percentile(
                    &rule,
                    entity_id,
                    entity_type,
                    protocol,
                    *percentile,
                    *threshold_ms,
                    *window_seconds,
                    &alert_key,
                )
                .await
            }
            AlertCondition::FailureRate {
                threshold,
                window_seconds,
            } => {
                self.check_failure_rate(
                    &rule,
                    entity_id,
                    entity_type,
                    protocol,
                    *threshold,
                    *window_seconds,
                    &alert_key,
                )
                .await
            }
            AlertCondition::SustainedDegradation {
                failure_count,
                window_seconds,
            } => {
                self.check_sustained_degradation(
                    &rule,
                    entity_id,
                    entity_type,
                    protocol,
                    *failure_count,
                    *window_seconds,
                    &alert_key,
                )
                .await
            }
        }
    }

    /// Check if a check is down
    async fn check_down(
        &mut self,
        rule: &AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
        alert_key: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let since = Utc::now() - Duration::seconds(60);
        let observations = self
            .storage
            .get_observations(
                entity_id,
                &format!("{}_latency_ms", protocol),
                since,
                None,
                Some(1),
            )
            .await?;

        if let Some(obs) = observations.first() {
            if obs.value == 0.0 {
                // Check is down (no latency recorded = failure)
                let eval = AlertEvaluation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    entity_id: entity_id.to_string(),
                    entity_type: entity_type.to_string(),
                    protocol: protocol.to_string(),
                    fired: true,
                    severity: rule.severity.clone(),
                    message: format!(
                        "Check {} for {} ({}) is down",
                        protocol, entity_id, entity_type
                    ),
                    evidence: serde_json::json!({
                        "condition": "down",
                        "protocol": protocol,
                        "entity_id": entity_id,
                        "entity_type": entity_type,
                    }),
                    evaluated_at: Utc::now(),
                };
                self.maybe_emit_event(&eval, alert_key).await?;
                return Ok(Some(eval));
            }
        }

        // Check if we had an active alert and it should be resolved
        if let Some(active) = self.active_alerts.get(alert_key) {
            if active.fired {
                let eval = AlertEvaluation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    entity_id: entity_id.to_string(),
                    entity_type: entity_type.to_string(),
                    protocol: protocol.to_string(),
                    fired: false,
                    severity: rule.severity.clone(),
                    message: format!(
                        "Check {} for {} ({}) recovered",
                        protocol, entity_id, entity_type
                    ),
                    evidence: serde_json::json!({
                        "condition": "recovered",
                        "protocol": protocol,
                        "entity_id": entity_id,
                        "entity_type": entity_type,
                    }),
                    evaluated_at: Utc::now(),
                };
                self.active_alerts.remove(alert_key);
                self.maybe_emit_event(&eval, alert_key).await?;
                return Ok(Some(eval));
            }
        }

        Ok(None)
    }

    /// Check if a check is up (recovery)
    async fn check_up(
        &mut self,
        rule: &AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
        alert_key: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let since = Utc::now() - Duration::seconds(60);
        let observations = self
            .storage
            .get_observations(
                entity_id,
                &format!("{}_latency_ms", protocol),
                since,
                None,
                Some(1),
            )
            .await?;

        if let Some(obs) = observations.first() {
            if obs.value > 0.0 {
                // Check is up
                if let Some(active) = self.active_alerts.get(alert_key) {
                    if active.fired {
                        let eval = AlertEvaluation {
                            rule_id: rule.id.clone(),
                            rule_name: rule.name.clone(),
                            entity_id: entity_id.to_string(),
                            entity_type: entity_type.to_string(),
                            protocol: protocol.to_string(),
                            fired: false,
                            severity: rule.severity.clone(),
                            message: format!(
                                "Check {} for {} ({}) recovered",
                                protocol, entity_id, entity_type
                            ),
                            evidence: serde_json::json!({
                                "condition": "recovered",
                                "protocol": protocol,
                                "entity_id": entity_id,
                                "entity_type": entity_type,
                            }),
                            evaluated_at: Utc::now(),
                        };
                        self.active_alerts.remove(alert_key);
                        self.maybe_emit_event(&eval, alert_key).await?;
                        return Ok(Some(eval));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Check latency threshold
    async fn check_latency_threshold(
        &mut self,
        rule: &AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
        threshold_ms: f64,
        alert_key: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let since = Utc::now() - Duration::seconds(60);
        let observations = self
            .storage
            .get_observations(
                entity_id,
                &format!("{}_latency_ms", protocol),
                since,
                None,
                Some(1),
            )
            .await?;

        if let Some(obs) = observations.first() {
            if obs.value > threshold_ms {
                let eval = AlertEvaluation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    entity_id: entity_id.to_string(),
                    entity_type: entity_type.to_string(),
                    protocol: protocol.to_string(),
                    fired: true,
                    severity: rule.severity.clone(),
                    message: format!(
                        "Latency for {} ({}) exceeded threshold: {:.2}ms > {:.2}ms",
                        entity_id, protocol, obs.value, threshold_ms
                    ),
                    evidence: serde_json::json!({
                        "condition": "latency_threshold",
                        "protocol": protocol,
                        "entity_id": entity_id,
                        "entity_type": entity_type,
                        "value": obs.value,
                        "threshold": threshold_ms,
                    }),
                    evaluated_at: Utc::now(),
                };
                self.maybe_emit_event(&eval, alert_key).await?;
                return Ok(Some(eval));
            } else if let Some(active) = self.active_alerts.get(alert_key) {
                if active.fired {
                    let eval = AlertEvaluation {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        entity_id: entity_id.to_string(),
                        entity_type: entity_type.to_string(),
                        protocol: protocol.to_string(),
                        fired: false,
                        severity: rule.severity.clone(),
                        message: format!(
                            "Latency for {} ({}) recovered: {:.2}ms <= {:.2}ms",
                            entity_id, protocol, obs.value, threshold_ms
                        ),
                        evidence: serde_json::json!({
                            "condition": "latency_recovered",
                            "protocol": protocol,
                            "entity_id": entity_id,
                            "entity_type": entity_type,
                            "value": obs.value,
                            "threshold": threshold_ms,
                        }),
                        evaluated_at: Utc::now(),
                    };
                    self.active_alerts.remove(alert_key);
                    self.maybe_emit_event(&eval, alert_key).await?;
                    return Ok(Some(eval));
                }
            }
        }

        Ok(None)
    }

    /// Check latency percentile
    #[allow(clippy::too_many_arguments)]
    async fn check_latency_percentile(
        &mut self,
        rule: &AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
        percentile: f64,
        threshold_ms: f64,
        window_seconds: i64,
        alert_key: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let since = Utc::now() - Duration::seconds(window_seconds);
        if let Some(percentile_value) = self
            .storage
            .compute_percentile(
                entity_id,
                &format!("{}_latency_ms", protocol),
                percentile,
                since,
                None,
            )
            .await?
        {
            if percentile_value > threshold_ms {
                let eval = AlertEvaluation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    entity_id: entity_id.to_string(),
                    entity_type: entity_type.to_string(),
                    protocol: protocol.to_string(),
                    fired: true,
                    severity: rule.severity.clone(),
                    message: format!(
                        "Latency p{} for {} ({}) exceeded threshold: {:.2}ms > {:.2}ms",
                        percentile, entity_id, protocol, percentile_value, threshold_ms
                    ),
                    evidence: serde_json::json!({
                        "condition": "latency_percentile",
                        "protocol": protocol,
                        "entity_id": entity_id,
                        "entity_type": entity_type,
                        "percentile": percentile,
                        "value": percentile_value,
                        "threshold": threshold_ms,
                        "window_seconds": window_seconds,
                    }),
                    evaluated_at: Utc::now(),
                };
                self.maybe_emit_event(&eval, alert_key).await?;
                return Ok(Some(eval));
            } else if let Some(active) = self.active_alerts.get(alert_key) {
                if active.fired {
                    let eval = AlertEvaluation {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        entity_id: entity_id.to_string(),
                        entity_type: entity_type.to_string(),
                        protocol: protocol.to_string(),
                        fired: false,
                        severity: rule.severity.clone(),
                        message: format!(
                            "Latency p{} for {} ({}) recovered: {:.2}ms <= {:.2}ms",
                            percentile, entity_id, protocol, percentile_value, threshold_ms
                        ),
                        evidence: serde_json::json!({
                            "condition": "latency_percentile_recovered",
                            "protocol": protocol,
                            "entity_id": entity_id,
                            "entity_type": entity_type,
                            "percentile": percentile,
                            "value": percentile_value,
                            "threshold": threshold_ms,
                        }),
                        evaluated_at: Utc::now(),
                    };
                    self.active_alerts.remove(alert_key);
                    self.maybe_emit_event(&eval, alert_key).await?;
                    return Ok(Some(eval));
                }
            }
        }
        Ok(None)
    }

    /// Check failure rate
    #[allow(clippy::too_many_arguments)]
    async fn check_failure_rate(
        &mut self,
        rule: &AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
        threshold: f64,
        window_seconds: i64,
        alert_key: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let since = Utc::now() - Duration::seconds(window_seconds);
        let stats = self
            .storage
            .get_availability_stats(entity_id, protocol, since, None)
            .await?;

        if stats.total_checks > 0 {
            let failure_rate = 1.0 - (stats.successful_checks as f64 / stats.total_checks as f64);
            if failure_rate > threshold {
                let eval = AlertEvaluation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    entity_id: entity_id.to_string(),
                    entity_type: entity_type.to_string(),
                    protocol: protocol.to_string(),
                    fired: true,
                    severity: rule.severity.clone(),
                    message: format!(
                        "Failure rate for {} ({}) exceeded threshold: {:.2}% > {:.2}%",
                        entity_id,
                        protocol,
                        failure_rate * 100.0,
                        threshold * 100.0
                    ),
                    evidence: serde_json::json!({
                        "condition": "failure_rate",
                        "protocol": protocol,
                        "entity_id": entity_id,
                        "entity_type": entity_type,
                        "failure_rate": failure_rate,
                        "threshold": threshold,
                        "total_checks": stats.total_checks,
                        "successful_checks": stats.successful_checks,
                        "window_seconds": window_seconds,
                    }),
                    evaluated_at: Utc::now(),
                };
                self.maybe_emit_event(&eval, alert_key).await?;
                return Ok(Some(eval));
            } else if let Some(active) = self.active_alerts.get(alert_key) {
                if active.fired {
                    let eval = AlertEvaluation {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        entity_id: entity_id.to_string(),
                        entity_type: entity_type.to_string(),
                        protocol: protocol.to_string(),
                        fired: false,
                        severity: rule.severity.clone(),
                        message: format!(
                            "Failure rate for {} ({}) recovered: {:.2}% <= {:.2}%",
                            entity_id,
                            protocol,
                            failure_rate * 100.0,
                            threshold * 100.0
                        ),
                        evidence: serde_json::json!({
                            "condition": "failure_rate_recovered",
                            "protocol": protocol,
                            "entity_id": entity_id,
                            "entity_type": entity_type,
                            "failure_rate": failure_rate,
                            "threshold": threshold,
                        }),
                        evaluated_at: Utc::now(),
                    };
                    self.active_alerts.remove(alert_key);
                    self.maybe_emit_event(&eval, alert_key).await?;
                    return Ok(Some(eval));
                }
            }
        }
        Ok(None)
    }

    /// Check sustained degradation (consecutive failures)
    #[allow(clippy::too_many_arguments)]
    async fn check_sustained_degradation(
        &mut self,
        rule: &AlertRule,
        entity_id: &str,
        entity_type: &str,
        protocol: &str,
        failure_count: u32,
        window_seconds: i64,
        alert_key: &str,
    ) -> Result<Option<AlertEvaluation>> {
        let since = Utc::now() - Duration::seconds(window_seconds);
        let observations = self
            .storage
            .get_observations(
                entity_id,
                &format!("{}_latency_ms", protocol),
                since,
                None,
                Some(failure_count as i64 + 10),
            )
            .await?;

        let mut consecutive_failures = 0;
        for obs in observations.iter().rev() {
            if obs.value == 0.0 {
                consecutive_failures += 1;
                if consecutive_failures >= failure_count {
                    let eval = AlertEvaluation {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        entity_id: entity_id.to_string(),
                        entity_type: entity_type.to_string(),
                        protocol: protocol.to_string(),
                        fired: true,
                        severity: rule.severity.clone(),
                        message: format!(
                            "Sustained degradation for {} ({}): {} consecutive failures",
                            entity_id, protocol, consecutive_failures
                        ),
                        evidence: serde_json::json!({
                            "condition": "sustained_degradation",
                            "protocol": protocol,
                            "entity_id": entity_id,
                            "entity_type": entity_type,
                            "consecutive_failures": consecutive_failures,
                            "threshold": failure_count,
                            "window_seconds": window_seconds,
                        }),
                        evaluated_at: Utc::now(),
                    };
                    self.maybe_emit_event(&eval, alert_key).await?;
                    return Ok(Some(eval));
                }
            } else {
                consecutive_failures = 0;
            }
        }

        // Check for recovery
        if let Some(active) = self.active_alerts.get(alert_key) {
            if active.fired {
                let eval = AlertEvaluation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    entity_id: entity_id.to_string(),
                    entity_type: entity_type.to_string(),
                    protocol: protocol.to_string(),
                    fired: false,
                    severity: rule.severity.clone(),
                    message: format!(
                        "Sustained degradation for {} ({}) recovered",
                        entity_id, protocol
                    ),
                    evidence: serde_json::json!({
                        "condition": "sustained_degradation_recovered",
                        "protocol": protocol,
                        "entity_id": entity_id,
                        "entity_type": entity_type,
                    }),
                    evaluated_at: Utc::now(),
                };
                self.active_alerts.remove(alert_key);
                self.maybe_emit_event(&eval, alert_key).await?;
                return Ok(Some(eval));
            }
        }

        Ok(None)
    }

    /// Emit event to EventBus if alert state changed
    async fn maybe_emit_event(&mut self, eval: &AlertEvaluation, alert_key: &str) -> Result<()> {
        let should_emit = match self.active_alerts.get(alert_key) {
            Some(active) => active.fired != eval.fired,
            None => eval.fired,
        };

        if should_emit {
            let _event_type = if eval.fired {
                EventType::ThresholdBreached
            } else {
                EventType::ThresholdRecovered
            };

            let _severity = match eval.severity {
                AlertSeverity::Info => Severity::Info,
                AlertSeverity::Warning => Severity::Warning,
                AlertSeverity::Critical => Severity::Critical,
            };

            let entity_type = match eval.entity_type.as_str() {
                "device" => EntityType::Device,
                "interface" => EntityType::Interface,
                "service" => EntityType::Service,
                _ => EntityType::Device,
            };

            let check_evidence = CheckEvidence {
                target: eval.entity_id.clone(),
                protocol: eval.protocol.clone(),
                success: !eval.fired,
                latency_ms: eval
                    .evidence
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as u128),
                error: if eval.fired {
                    Some(eval.message.clone())
                } else {
                    None
                },
            };

            let event = self
                .event_bus
                .emit_check_completed(eval.entity_id.clone(), entity_type, check_evidence)
                .await;

            if let Err(e) = event {
                tracing::error!("Failed to emit alert event: {}", e);
            } else {
                if eval.fired {
                    self.active_alerts
                        .insert(alert_key.to_string(), eval.clone());
                } else {
                    self.active_alerts.remove(alert_key);
                }
            }
        }

        Ok(())
    }

    /// Run evaluation for all monitored entities
    pub async fn run_evaluation(&mut self) -> Result<()> {
        // Get all unique entity/protocol combinations from recent observations
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT device_id, metric FROM observations
            WHERE observed_at >= datetime('now', '-5 minutes')
            AND metric LIKE '%_latency_ms'
            "#,
        )
        .fetch_all(&self.storage.pool)
        .await?;

        for row in rows {
            let device_id: String = row.get("device_id");
            let metric: String = row.get("metric");
            if let Some(protocol) = metric.strip_suffix("_latency_ms") {
                let _ = self.evaluate_entity(&device_id, "device", protocol).await;
            }
        }

        Ok(())
    }

    /// Get active alerts
    pub fn get_active_alerts(&self) -> Vec<AlertEvaluation> {
        self.active_alerts.values().cloned().collect()
    }
}

/// Alert engine configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlertEngineConfig {
    pub enabled: Option<bool>,
    pub interval: Option<u64>,
    pub rules: Vec<AlertRule>,
}

impl From<ConfigAlertConfig> for AlertEngineConfig {
    fn from(cfg: ConfigAlertConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            interval: cfg.interval,
            rules: cfg.rules,
        }
    }
}
