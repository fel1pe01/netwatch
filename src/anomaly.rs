use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::AnomalyConfig as ConfigAnomalyConfig;
use crate::events::{AnomalyEvidence as EventAnomalyEvidence, EntityType, EventBus};
use crate::storage::Storage;

/// Anomaly detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyConfig {
    /// Minimum number of samples required before anomaly detection runs
    pub min_samples: usize,
    /// Lookback window for baseline computation
    pub lookback_window_seconds: i64,
    /// Sensitivity multiplier for z-score threshold (higher = less sensitive)
    pub sensitivity: f64,
    /// Minimum z-score to trigger anomaly (default 3.0 for 99.7% confidence)
    pub min_z_score: f64,
    /// Enable spike detection
    pub enable_spike_detection: bool,
    /// Enable gradual drift detection
    pub enable_drift_detection: bool,
    /// Enable pattern break detection
    pub enable_pattern_break_detection: bool,
    /// Cooldown period in seconds to prevent duplicate anomaly events for the same entity/metric
    pub anomaly_cooldown_seconds: i64,
    /// Minimum absolute change for constant series (zero stddev) spike detection
    pub min_absolute_change: f64,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            min_samples: 10,
            lookback_window_seconds: 3600, // 1 hour
            sensitivity: 1.0,
            min_z_score: 3.0,
            enable_spike_detection: true,
            enable_drift_detection: true,
            enable_pattern_break_detection: false,
            anomaly_cooldown_seconds: 300, // 5 minutes
            min_absolute_change: 10.0,     // 10ms default for latency
        }
    }
}

impl From<ConfigAnomalyConfig> for AnomalyConfig {
    fn from(cfg: ConfigAnomalyConfig) -> Self {
        Self {
            min_samples: cfg.min_samples.unwrap_or(10),
            lookback_window_seconds: cfg.lookback_window_seconds.unwrap_or(3600),
            sensitivity: cfg.sensitivity.unwrap_or(1.0),
            min_z_score: cfg.min_z_score.unwrap_or(3.0),
            enable_spike_detection: cfg.enable_spike_detection.unwrap_or(true),
            enable_drift_detection: cfg.enable_drift_detection.unwrap_or(true),
            enable_pattern_break_detection: cfg.enable_pattern_break_detection.unwrap_or(false),
            anomaly_cooldown_seconds: cfg.anomaly_cooldown_seconds.unwrap_or(300),
            min_absolute_change: cfg.min_absolute_change.unwrap_or(10.0),
        }
    }
}

/// Anomaly types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    /// Sudden spike in value
    Spike,
    /// Gradual drift from baseline
    Drift,
    /// Pattern break (e.g., periodicity change)
    PatternBreak,
}

impl std::fmt::Display for AnomalyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnomalyType::Spike => write!(f, "spike"),
            AnomalyType::Drift => write!(f, "drift"),
            AnomalyType::PatternBreak => write!(f, "pattern_break"),
        }
    }
}

/// Anomaly severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Info,
    Warning,
    Critical,
}

impl std::fmt::Display for AnomalySeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnomalySeverity::Info => write!(f, "info"),
            AnomalySeverity::Warning => write!(f, "warning"),
            AnomalySeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Anomaly evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyEvidence {
    /// The entity being monitored
    pub entity_id: String,
    /// The metric being analyzed
    pub metric: String,
    /// The protocol (icmp, tcp, dns, http, tls)
    pub protocol: String,
    /// Current value that triggered the anomaly
    pub current_value: f64,
    /// Baseline mean
    pub baseline_mean: f64,
    /// Baseline standard deviation
    pub baseline_stddev: f64,
    /// Z-score of current value
    pub z_score: f64,
    /// Number of samples in baseline
    pub sample_count: usize,
    /// Time window of baseline
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    /// Anomaly type
    pub anomaly_type: AnomalyType,
}

/// Anomaly event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyEvent {
    pub id: String,
    pub rule_id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub metric: String,
    pub protocol: String,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub evidence: AnomalyEvidence,
    pub detected_at: DateTime<Utc>,
    pub confidence: f64,
    pub description: String,
    pub correlation_id: Option<String>,
}

impl AnomalyEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rule_id: String,
        entity_id: String,
        entity_type: String,
        metric: String,
        protocol: String,
        anomaly_type: AnomalyType,
        severity: AnomalySeverity,
        evidence: AnomalyEvidence,
        confidence: f64,
        description: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            rule_id,
            entity_id,
            entity_type,
            metric,
            protocol,
            anomaly_type,
            severity,
            evidence,
            detected_at: Utc::now(),
            confidence,
            description,
            correlation_id: None,
        }
    }

    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
}

/// Baseline statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub entity_id: String,
    pub metric: String,
    pub protocol: String,
    pub mean: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
    pub sample_count: usize,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub computed_at: DateTime<Utc>,
}

impl Baseline {
    pub fn z_score(&self, value: f64) -> f64 {
        if self.stddev > 0.0 {
            (value - self.mean) / self.stddev
        } else {
            0.0
        }
    }
}

/// Anomaly detector
pub struct AnomalyDetector {
    storage: Arc<Storage>,
    config: AnomalyConfig,
    baselines: HashMap<String, Baseline>,
    /// Cache of recently emitted anomalies for deduplication
    /// Key: "entity_id:metric:protocol:anomaly_type", Value: timestamp of last emission
    recent_anomalies: HashMap<String, DateTime<Utc>>,
    enabled: bool,
}

impl AnomalyDetector {
    pub fn new(storage: Arc<Storage>, config: AnomalyConfig) -> Self {
        Self {
            storage,
            config,
            baselines: HashMap::new(),
            recent_anomalies: HashMap::new(),
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_config(&mut self, config: AnomalyConfig) {
        self.config = config;
    }

    pub fn get_config(&self) -> &AnomalyConfig {
        &self.config
    }

    /// Compute baseline for an entity/metric/protocol combination
    pub async fn compute_baseline(
        &mut self,
        entity_id: &str,
        metric: &str,
        protocol: &str,
    ) -> Result<Option<Baseline>> {
        let key = format!("{}:{}:{}", entity_id, metric, protocol);
        let window_end = Utc::now();
        let window_start = window_end - Duration::seconds(self.config.lookback_window_seconds);

        let values = self
            .storage
            .get_latency_observations(entity_id, protocol, window_start, Some(window_end))
            .await?;

        if values.len() < self.config.min_samples {
            return Ok(None);
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let stddev = variance.sqrt();
        let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

        let baseline = Baseline {
            entity_id: entity_id.to_string(),
            metric: metric.to_string(),
            protocol: protocol.to_string(),
            mean,
            stddev,
            min,
            max,
            sample_count: values.len(),
            window_start,
            window_end,
            computed_at: Utc::now(),
        };

        self.baselines.insert(key, baseline.clone());
        Ok(Some(baseline))
    }

    /// Get cached baseline or compute if not present
    pub async fn get_or_compute_baseline(
        &mut self,
        entity_id: &str,
        metric: &str,
        protocol: &str,
    ) -> Result<Option<Baseline>> {
        let key = format!("{}:{}:{}", entity_id, metric, protocol);
        if let Some(baseline) = self.baselines.get(&key) {
            // Check if baseline is still fresh (within lookback window)
            let age = Utc::now() - baseline.computed_at;
            if age < Duration::seconds(self.config.lookback_window_seconds) {
                return Ok(Some(baseline.clone()));
            }
        }
        self.compute_baseline(entity_id, metric, protocol).await
    }

    /// Check for anomalies in a new observation
    #[allow(clippy::too_many_arguments)]
    pub async fn check_observation(
        &mut self,
        entity_id: &str,
        metric: &str,
        protocol: &str,
        value: f64,
        entity_type: &str,
    ) -> Result<Vec<AnomalyEvent>> {
        if !self.enabled {
            return Ok(vec![]);
        }

        let baseline = match self
            .get_or_compute_baseline(entity_id, metric, protocol)
            .await?
        {
            Some(b) => b,
            None => return Ok(vec![]), // Not enough data
        };

        let mut anomalies = Vec::new();

        // Spike detection
        if self.config.enable_spike_detection {
            if let Some(anomaly) = self
                .check_spike(entity_id, entity_type, metric, protocol, value, &baseline)
                .await?
            {
                anomalies.push(anomaly);
            }
        }

        // Drift detection
        if self.config.enable_drift_detection {
            if let Some(anomaly) = self
                .check_drift(entity_id, entity_type, metric, protocol, value, &baseline)
                .await?
            {
                anomalies.push(anomaly);
            }
        }

        Ok(anomalies)
    }

    /// Check for sudden spike anomaly
    async fn check_spike(
        &self,
        entity_id: &str,
        entity_type: &str,
        metric: &str,
        protocol: &str,
        value: f64,
        baseline: &Baseline,
    ) -> Result<Option<AnomalyEvent>> {
        let z_score = baseline.z_score(value);
        let threshold = self.config.min_z_score * self.config.sensitivity;

        // Handle constant series (zero or near-zero stddev)
        // Use absolute change threshold instead of z-score
        let is_constant_series = baseline.stddev < f64::EPSILON;
        let abs_change = (value - baseline.mean).abs();

        let (_is_anomaly, effective_z_score, severity, _direction, description) =
            if is_constant_series {
                // For constant series, use absolute change threshold
                if abs_change >= self.config.min_absolute_change {
                    let direction = if value > baseline.mean {
                        "increase"
                    } else {
                        "decrease"
                    };
                    let desc = format!(
                    "Sudden {} in {} ({}): absolute change {:.2} (threshold {:.2}), value {:.2} vs constant baseline {:.2}",
                    direction, metric, protocol, abs_change, self.config.min_absolute_change, value, baseline.mean
                );
                    (
                        true,
                        abs_change / self.config.min_absolute_change,
                        AnomalySeverity::Warning,
                        direction,
                        desc,
                    )
                } else {
                    return Ok(None);
                }
            } else if z_score.abs() >= threshold {
                let severity = if z_score.abs() >= threshold * 2.0 {
                    AnomalySeverity::Critical
                } else if z_score.abs() >= threshold * 1.5 {
                    AnomalySeverity::Warning
                } else {
                    AnomalySeverity::Info
                };
                let direction = if z_score > 0.0 {
                    "increase"
                } else {
                    "decrease"
                };
                let desc = format!(
                "Sudden {} in {} ({}): z-score {:.2} (threshold {:.2}), value {:.2} vs baseline {:.2}±{:.2}",
                direction, metric, protocol, z_score.abs(), threshold, value, baseline.mean, baseline.stddev
            );
                (true, z_score, severity, direction, desc)
            } else {
                return Ok(None);
            };

        let evidence = AnomalyEvidence {
            entity_id: entity_id.to_string(),
            metric: metric.to_string(),
            protocol: protocol.to_string(),
            current_value: value,
            baseline_mean: baseline.mean,
            baseline_stddev: baseline.stddev,
            z_score: effective_z_score,
            sample_count: baseline.sample_count,
            window_start: baseline.window_start,
            window_end: baseline.window_end,
            anomaly_type: AnomalyType::Spike,
        };

        let rule_id = format!("spike_{}_{}", metric, protocol);

        Ok(Some(AnomalyEvent::new(
            rule_id,
            entity_id.to_string(),
            entity_type.to_string(),
            metric.to_string(),
            protocol.to_string(),
            AnomalyType::Spike,
            severity,
            evidence,
            (effective_z_score.abs() / threshold).min(1.0),
            description,
        )))
    }

    /// Check for gradual drift anomaly
    /// Uses a separate recent window (last 10 minutes) that does not overlap with the baseline window
    async fn check_drift(
        &self,
        entity_id: &str,
        entity_type: &str,
        metric: &str,
        protocol: &str,
        value: f64,
        baseline: &Baseline,
    ) -> Result<Option<AnomalyEvent>> {
        // Skip drift detection for constant series
        if baseline.stddev < f64::EPSILON {
            return Ok(None);
        }

        let z_score = baseline.z_score(value);
        let threshold = self.config.min_z_score * self.config.sensitivity * 0.5; // Lower threshold for drift

        // Use a recent window that ENDS at the baseline window_start to avoid contamination
        // Baseline covers [window_start, window_end], so recent window should be before window_start
        let recent_end = baseline.window_start;
        let recent_start = recent_end - Duration::minutes(10);

        let recent_values = self
            .storage
            .get_latency_observations(entity_id, protocol, recent_start, Some(recent_end))
            .await?;

        if recent_values.len() < 5 {
            return Ok(None);
        }

        // Check if recent values consistently deviate in the same direction from baseline
        let deviations: Vec<f64> = recent_values.iter().map(|v| baseline.z_score(*v)).collect();
        let positive_count = deviations.iter().filter(|d| **d > threshold).count();
        let negative_count = deviations.iter().filter(|d| **d < -threshold).count();

        if positive_count >= 4 || negative_count >= 4 {
            let severity = if positive_count >= 4 && negative_count >= 4 {
                AnomalySeverity::Critical
            } else {
                AnomalySeverity::Warning
            };

            let direction = if positive_count > negative_count {
                "upward"
            } else {
                "downward"
            };
            let description = format!(
                "Gradual {} drift in {} ({}): {}/{} recent samples deviate consistently (z-score threshold {:.2}), current z-score {:.2}",
                direction, metric, protocol, positive_count.max(negative_count), recent_values.len(), threshold, z_score
            );

            let evidence = AnomalyEvidence {
                entity_id: entity_id.to_string(),
                metric: metric.to_string(),
                protocol: protocol.to_string(),
                current_value: value,
                baseline_mean: baseline.mean,
                baseline_stddev: baseline.stddev,
                z_score,
                sample_count: baseline.sample_count,
                window_start: baseline.window_start,
                window_end: baseline.window_end,
                anomaly_type: AnomalyType::Drift,
            };

            let rule_id = format!("drift_{}_{}", metric, protocol);

            Ok(Some(AnomalyEvent::new(
                rule_id,
                entity_id.to_string(),
                entity_type.to_string(),
                metric.to_string(),
                protocol.to_string(),
                AnomalyType::Drift,
                severity,
                evidence,
                0.8,
                description,
            )))
        } else {
            Ok(None)
        }
    }

    /// Clear cached baselines (call periodically to force recomputation)
    pub fn clear_baselines(&mut self) {
        self.baselines.clear();
    }

    /// Clear the anomaly deduplication cache
    pub fn clear_anomaly_cache(&mut self) {
        self.recent_anomalies.clear();
    }

    /// Clean up expired entries from the anomaly deduplication cache
    fn cleanup_anomaly_cache(&mut self) {
        let now = Utc::now();
        let cooldown = Duration::seconds(self.config.anomaly_cooldown_seconds);
        self.recent_anomalies
            .retain(|_, &mut ts| now - ts < cooldown);
    }

    /// Check if an anomaly was recently emitted (for deduplication)
    pub fn is_recent_anomaly(
        &self,
        entity_id: &str,
        metric: &str,
        protocol: &str,
        anomaly_type: &AnomalyType,
    ) -> bool {
        let key = format!("{}:{}:{}:{}", entity_id, metric, protocol, anomaly_type);
        if let Some(&last_emitted) = self.recent_anomalies.get(&key) {
            let cooldown = Duration::seconds(self.config.anomaly_cooldown_seconds);
            Utc::now() - last_emitted < cooldown
        } else {
            false
        }
    }

    /// Record an anomaly emission for deduplication
    fn record_anomaly_emission(
        &mut self,
        entity_id: &str,
        metric: &str,
        protocol: &str,
        anomaly_type: &AnomalyType,
    ) {
        let key = format!("{}:{}:{}:{}", entity_id, metric, protocol, anomaly_type);
        self.recent_anomalies.insert(key, Utc::now());
    }

    /// Check for pattern break anomaly (e.g., sudden change in periodicity or trend)
    /// Compares recent trend with historical trend
    pub async fn check_pattern_break(
        &self,
        entity_id: &str,
        entity_type: &str,
        metric: &str,
        protocol: &str,
        baseline: &Baseline,
    ) -> Result<Option<AnomalyEvent>> {
        // Need at least 2 windows of data for pattern comparison
        let window_duration = Duration::seconds(self.config.lookback_window_seconds);
        let recent_end = baseline.window_start;
        let recent_start = recent_end - window_duration;

        // Get historical window (before recent window)
        let historical_end = recent_start;
        let historical_start = historical_end - window_duration;

        let recent_values = self
            .storage
            .get_latency_observations(entity_id, protocol, recent_start, Some(recent_end))
            .await?;

        let historical_values = self
            .storage
            .get_latency_observations(entity_id, protocol, historical_start, Some(historical_end))
            .await?;

        if recent_values.len() < self.config.min_samples
            || historical_values.len() < self.config.min_samples
        {
            return Ok(None);
        }

        // Compute statistics for both windows
        let recent_mean = recent_values.iter().sum::<f64>() / recent_values.len() as f64;
        let historical_mean =
            historical_values.iter().sum::<f64>() / historical_values.len() as f64;

        let recent_stddev = {
            let variance = recent_values
                .iter()
                .map(|v| (v - recent_mean).powi(2))
                .sum::<f64>()
                / recent_values.len() as f64;
            variance.sqrt()
        };
        let historical_stddev = {
            let variance = historical_values
                .iter()
                .map(|v| (v - historical_mean).powi(2))
                .sum::<f64>()
                / historical_values.len() as f64;
            variance.sqrt()
        };

        // Detect significant change in mean or variance
        let mean_change_ratio = if historical_mean != 0.0 {
            (recent_mean - historical_mean).abs() / historical_mean.abs()
        } else {
            0.0
        };

        let stddev_change_ratio = if historical_stddev > 0.0 {
            (recent_stddev - historical_stddev).abs() / historical_stddev
        } else {
            0.0
        };

        // Threshold for pattern break: 50% change in mean or 100% change in stddev
        const MEAN_CHANGE_THRESHOLD: f64 = 0.5;
        const STDDEV_CHANGE_THRESHOLD: f64 = 1.0;

        if mean_change_ratio >= MEAN_CHANGE_THRESHOLD
            || stddev_change_ratio >= STDDEV_CHANGE_THRESHOLD
        {
            let change_type = if mean_change_ratio >= MEAN_CHANGE_THRESHOLD
                && stddev_change_ratio >= STDDEV_CHANGE_THRESHOLD
            {
                "mean and variance"
            } else if mean_change_ratio >= MEAN_CHANGE_THRESHOLD {
                "mean"
            } else {
                "variance"
            };

            let direction = if recent_mean > historical_mean {
                "increase"
            } else {
                "decrease"
            };
            let description = format!(
                "Pattern break detected in {} ({}): significant change in {} ({} change, {:.1}% mean change, {:.1}% stddev change)",
                metric, protocol, change_type, direction, mean_change_ratio * 100.0, stddev_change_ratio * 100.0
            );

            let evidence = AnomalyEvidence {
                entity_id: entity_id.to_string(),
                metric: metric.to_string(),
                protocol: protocol.to_string(),
                current_value: recent_mean,
                baseline_mean: historical_mean,
                baseline_stddev: historical_stddev,
                z_score: mean_change_ratio.max(stddev_change_ratio),
                sample_count: recent_values.len() + historical_values.len(),
                window_start: historical_start,
                window_end: recent_end,
                anomaly_type: AnomalyType::PatternBreak,
            };

            let rule_id = format!("pattern_break_{}_{}", metric, protocol);

            Ok(Some(AnomalyEvent::new(
                rule_id,
                entity_id.to_string(),
                entity_type.to_string(),
                metric.to_string(),
                protocol.to_string(),
                AnomalyType::PatternBreak,
                AnomalySeverity::Warning,
                evidence,
                (mean_change_ratio.max(stddev_change_ratio)).min(1.0),
                description,
            )))
        } else {
            Ok(None)
        }
    }

    /// Convert AnomalyEvent to EventBus AnomalyEvidence
    fn anomaly_event_to_evidence(&self, event: &AnomalyEvent) -> EventAnomalyEvidence {
        EventAnomalyEvidence {
            entity_id: event.entity_id.clone(),
            entity_type: event.entity_type.clone(),
            metric: event.metric.clone(),
            protocol: event.protocol.clone(),
            anomaly_type: event.anomaly_type.to_string(),
            severity: event.severity.to_string(),
            current_value: event.evidence.current_value,
            baseline_mean: event.evidence.baseline_mean,
            baseline_stddev: event.evidence.baseline_stddev,
            z_score: event.evidence.z_score,
            sample_count: event.evidence.sample_count,
            window_start: event.evidence.window_start.to_rfc3339(),
            window_end: event.evidence.window_end.to_rfc3339(),
            rule_id: event.rule_id.clone(),
            confidence: event.confidence,
            description: event.description.clone(),
        }
    }

    /// Get all entity/protocol combinations that have observations
    async fn get_monitored_entities(&self) -> Result<Vec<(String, String)>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT device_id, metric FROM observations
            WHERE observed_at >= datetime('now', '-1 hour')
            "#,
        )
        .fetch_all(&self.storage.pool)
        .await?;

        let mut entities = Vec::new();
        for row in rows {
            let device_id: String = row.get("device_id");
            let metric: String = row.get("metric");
            // Extract protocol from metric (e.g., "icmp_latency_ms" -> "icmp")
            if let Some(protocol) = metric.strip_suffix("_latency_ms") {
                entities.push((device_id, protocol.to_string()));
            }
        }
        Ok(entities)
    }

    /// Run anomaly analysis on all monitored entities and emit events
    pub async fn analyze_and_emit(&mut self, event_bus: &EventBus) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        // Clean up expired anomaly cache entries
        self.cleanup_anomaly_cache();

        let entities = self.get_monitored_entities().await?;
        let now = Utc::now();

        for (entity_id, protocol) in entities {
            let metric = format!("{}_latency_ms", protocol);

            // Compute baseline from the lookback window (excluding the most recent 5 minutes)
            // Baseline window: [now - lookback_window, now - 5min]
            let baseline_end = now - Duration::minutes(5);
            let baseline_start =
                baseline_end - Duration::seconds(self.config.lookback_window_seconds);

            // Check window: [now - 5min, now]
            let check_start = now - Duration::minutes(5);
            let check_end = now;

            // Get baseline observations
            let baseline_obs = self
                .storage
                .get_latency_observations_with_timestamps(
                    &entity_id,
                    &protocol,
                    baseline_start,
                    Some(baseline_end),
                )
                .await?;

            let baseline_values: Vec<f64> = baseline_obs.iter().map(|o| o.value).collect();

            if baseline_values.len() < self.config.min_samples {
                continue;
            }

            // Compute baseline statistics
            let mean = baseline_values.iter().sum::<f64>() / baseline_values.len() as f64;
            let variance = baseline_values
                .iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>()
                / baseline_values.len() as f64;
            let stddev = variance.sqrt();
            let min = baseline_values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
            let max = baseline_values
                .iter()
                .fold(f64::NEG_INFINITY, |a, &b| a.max(b));

            let baseline = Baseline {
                entity_id: entity_id.clone(),
                metric: metric.clone(),
                protocol: protocol.clone(),
                mean,
                stddev,
                min,
                max,
                sample_count: baseline_values.len(),
                window_start: baseline_start,
                window_end: baseline_end,
                computed_at: Utc::now(),
            };

            // Cache the baseline
            let key = format!("{}:{}:{}", entity_id, metric, protocol);
            self.baselines.insert(key, baseline.clone());

            // Get recent observations to check
            let recent_values = self
                .storage
                .get_latency_observations(&entity_id, &protocol, check_start, Some(check_end))
                .await?;

            if recent_values.is_empty() {
                continue;
            }

            // Check each recent observation for anomalies
            for value in recent_values {
                let mut anomalies = Vec::new();

                // Spike detection
                if self.config.enable_spike_detection {
                    if let Some(anomaly) = self
                        .check_spike(&entity_id, "device", &metric, &protocol, value, &baseline)
                        .await?
                    {
                        // Deduplication check
                        if !self.is_recent_anomaly(
                            &entity_id,
                            &metric,
                            &protocol,
                            &AnomalyType::Spike,
                        ) {
                            anomalies.push(anomaly);
                        }
                    }
                }

                // Drift detection
                if self.config.enable_drift_detection {
                    if let Some(anomaly) = self
                        .check_drift(&entity_id, "device", &metric, &protocol, value, &baseline)
                        .await?
                    {
                        if !self.is_recent_anomaly(
                            &entity_id,
                            &metric,
                            &protocol,
                            &AnomalyType::Drift,
                        ) {
                            anomalies.push(anomaly);
                        }
                    }
                }

                // Pattern break detection (run once per entity, not per observation)
                if self.config.enable_pattern_break_detection {
                    if let Some(anomaly) = self
                        .check_pattern_break(&entity_id, "device", &metric, &protocol, &baseline)
                        .await?
                    {
                        if !self.is_recent_anomaly(
                            &entity_id,
                            &metric,
                            &protocol,
                            &AnomalyType::PatternBreak,
                        ) {
                            anomalies.push(anomaly);
                        }
                    }
                }

                // Emit events for each anomaly
                for anomaly in anomalies {
                    let evidence = self.anomaly_event_to_evidence(&anomaly);
                    let entity_type = match anomaly.entity_type.as_str() {
                        "device" => EntityType::Device,
                        "interface" => EntityType::Interface,
                        "service" => EntityType::Service,
                        _ => EntityType::Device,
                    };
                    // Record emission for deduplication
                    self.record_anomaly_emission(
                        &anomaly.entity_id,
                        &anomaly.metric,
                        &anomaly.protocol,
                        &anomaly.anomaly_type,
                    );

                    if let Err(e) = event_bus
                        .emit_anomaly_detected(anomaly.entity_id.clone(), entity_type, evidence)
                        .await
                    {
                        tracing::error!("Failed to emit anomaly event: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}
