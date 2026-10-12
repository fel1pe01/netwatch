use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

use crate::storage::Storage;

/// Capacity planning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityConfig {
    /// Minimum number of samples required for trend analysis
    pub min_samples: usize,
    /// Lookback window for trend analysis
    pub lookback_window_seconds: i64,
    /// Saturation threshold percentage (e.g., 80.0 for 80%)
    pub saturation_threshold_pct: f64,
    /// Confidence level for projections (e.g., 0.95 for 95%)
    pub confidence_level: f64,
    /// Enable capacity planning
    pub enabled: bool,
    /// Nominal capacity for resources (optional, per metric/protocol)
    /// If not set, max observed value is used as proxy with a warning
    pub nominal_capacity: Option<f64>,
}

impl Default for CapacityConfig {
    fn default() -> Self {
        Self {
            min_samples: 10,
            lookback_window_seconds: 30 * 24 * 3600, // 30 days
            saturation_threshold_pct: 80.0,
            confidence_level: 0.95,
            enabled: true,
            nominal_capacity: None,
        }
    }
}

/// Resource type for capacity planning
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    InterfaceUtilization,
    CpuUtilization,
    MemoryUtilization,
    DiskUtilization,
    Custom(String),
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::InterfaceUtilization => write!(f, "interface_utilization"),
            ResourceType::CpuUtilization => write!(f, "cpu_utilization"),
            ResourceType::MemoryUtilization => write!(f, "memory_utilization"),
            ResourceType::DiskUtilization => write!(f, "disk_utilization"),
            ResourceType::Custom(s) => write!(f, "custom_{}", s),
        }
    }
}

/// Capacity trend analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub resource_id: String,
    pub resource_type: ResourceType,
    pub entity_id: String,
    pub metric: String,
    pub protocol: String,
    /// Linear regression slope (rate of change per second)
    pub slope: f64,
    /// Linear regression intercept
    pub intercept: f64,
    /// Coefficient of determination (R²)
    pub r_squared: f64,
    /// Standard error of the estimate
    pub std_error: f64,
    /// Number of samples used
    pub sample_count: usize,
    /// Time window start
    pub window_start: DateTime<Utc>,
    /// Time window end
    pub window_end: DateTime<Utc>,
    /// Projected time to reach saturation threshold (if slope > 0)
    pub time_to_saturation: Option<DateTime<Utc>>,
    /// Current value
    pub current_value: f64,
    /// Saturation threshold value
    pub saturation_threshold: f64,
    /// Confidence level
    pub confidence_level: f64,
    /// Analysis timestamp
    pub analyzed_at: DateTime<Utc>,
    /// Warning: insufficient data for reliable projection
    pub insufficient_data_warning: bool,
    /// Source of capacity reference: "nominal_capacity" or "max_observed_proxy"
    pub capacity_source: String,
}

/// Capacity planning report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityReport {
    pub generated_at: DateTime<Utc>,
    pub analysis_period_days: i64,
    pub trends: Vec<TrendAnalysis>,
    /// Resources sorted by time to saturation (most urgent first)
    pub critical_resources: Vec<TrendAnalysis>,
    /// Resources with insufficient data
    pub insufficient_data: Vec<String>,
    /// Configuration used for this analysis
    pub config_used: CapacityConfig,
}

/// Capacity planner
pub struct CapacityPlanner {
    storage: Arc<Storage>,
    config: CapacityConfig,
}

impl CapacityPlanner {
    pub fn new(storage: Arc<Storage>, config: CapacityConfig) -> Self {
        Self { storage, config }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn set_config(&mut self, config: CapacityConfig) {
        self.config = config;
    }

    pub fn get_config(&self) -> &CapacityConfig {
        &self.config
    }

    /// Analyze capacity trends for all resources with sufficient data
    pub async fn analyze_capacity(&mut self) -> Result<CapacityReport> {
        if !self.config.enabled {
            return Ok(CapacityReport {
                generated_at: Utc::now(),
                analysis_period_days: self.config.lookback_window_seconds / (24 * 3600),
                trends: vec![],
                critical_resources: vec![],
                insufficient_data: vec!["Capacity planning disabled".to_string()],
                config_used: self.config.clone(),
            });
        }

        let window_end = Utc::now();
        let window_start = window_end - Duration::seconds(self.config.lookback_window_seconds);

        // Get all unique device/metric combinations from observations
        let resource_ids = self.get_resource_ids().await?;

        let mut trends = Vec::new();
        let mut insufficient_data = Vec::new();

        for resource_id in resource_ids {
            // Parse resource_id to get entity_id, metric, protocol
            let parts: Vec<&str> = resource_id.split(':').collect();
            if parts.len() < 3 {
                continue;
            }
            let entity_id = parts[0];
            let metric = parts[1];
            let protocol = parts[2];

            // Get latency observations with timestamps for trend analysis (single query)
            let observations = self
                .storage
                .get_latency_observations_with_timestamps(
                    entity_id,
                    protocol,
                    window_start,
                    Some(window_end),
                )
                .await?;

            let values: Vec<f64> = observations.iter().map(|o| o.value).collect();

            if values.len() < self.config.min_samples {
                insufficient_data.push(format!(
                    "{}:{} ({} samples)",
                    entity_id,
                    metric,
                    values.len()
                ));
                continue;
            }

            // Perform linear regression using the observations directly
            let trend_result = self
                .perform_linear_regression(
                    entity_id,
                    metric,
                    protocol,
                    &observations,
                    window_start,
                    window_end,
                )
                .await;

            match trend_result {
                Ok(trend) => {
                    if trend.insufficient_data_warning {
                        insufficient_data.push(format!(
                            "{}:{} (regression R²={:.3})",
                            entity_id, metric, trend.r_squared
                        ));
                    } else {
                        trends.push(trend);
                    }
                }
                Err(e) => {
                    // Handle regression errors gracefully (e.g., identical timestamps)
                    insufficient_data.push(format!(
                        "{}:{} (regression error: {})",
                        entity_id, metric, e
                    ));
                }
            }
        }

        // Sort critical resources by time to saturation (most urgent first)
        let mut critical_resources = trends
            .iter()
            .filter(|t| t.time_to_saturation.is_some())
            .cloned()
            .collect::<Vec<_>>();
        critical_resources.sort_by(|a, b| {
            let far_future = Utc::now() + Duration::days(365 * 100); // 100 years in the future
            a.time_to_saturation
                .unwrap_or(far_future)
                .cmp(&b.time_to_saturation.unwrap_or(far_future))
        });

        let insufficient_data_list = insufficient_data.to_vec();

        Ok(CapacityReport {
            generated_at: Utc::now(),
            analysis_period_days: self.config.lookback_window_seconds / (24 * 3600),
            trends: trends.clone(),
            critical_resources,
            insufficient_data: insufficient_data_list,
            config_used: self.config.clone(),
        })
    }

    /// Get all unique resource IDs (entity_id:metric:protocol) from observations
    async fn get_resource_ids(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT device_id, metric FROM observations
            WHERE observed_at >= datetime('now', '-30 days')
            "#,
        )
        .fetch_all(&self.storage.pool)
        .await?;

        let mut resource_ids = Vec::new();
        for row in rows {
            let device_id: String = row.get("device_id");
            let metric: String = row.get("metric");
            // Extract protocol from metric (e.g., "icmp_latency_ms" -> "icmp")
            if let Some(protocol) = metric.strip_suffix("_latency_ms") {
                resource_ids.push(format!("{}:{}:{}", device_id, metric, protocol));
            }
        }
        Ok(resource_ids)
    }

    /// Perform linear regression on time series data
    #[allow(clippy::too_many_arguments)]
    async fn perform_linear_regression(
        &self,
        entity_id: &str,
        metric: &str,
        protocol: &str,
        observations: &[crate::storage::Observation],
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<TrendAnalysis> {
        let n = observations.len();
        if n < 2 {
            return Err(anyhow::anyhow!("Insufficient data points for regression"));
        }

        // Extract values and timestamps from observations (already ordered by observed_at ASC)
        let y: Vec<f64> = observations.iter().map(|o| o.value).collect();

        // Use actual timestamps (seconds since window_start) as x-values
        let x: Vec<f64> = observations
            .iter()
            .map(|obs| (obs.observed_at - window_start).num_seconds() as f64)
            .collect();

        // Verify temporal ordering (observations should be ASC by observed_at)
        // If out of order, we still proceed but log a warning via the insufficient_data_warning flag
        let is_ordered = x.windows(2).all(|w| w[0] <= w[1]);

        // Calculate linear regression: y = a + bx
        let sum_x: f64 = x.iter().sum();
        let sum_y: f64 = y.iter().sum();
        let sum_xy: f64 = x.iter().zip(y.iter()).map(|(xi, yi)| xi * yi).sum();
        let sum_x2: f64 = x.iter().map(|xi| xi * xi).sum();

        let n_f = n as f64;
        let denominator = n_f * sum_x2 - sum_x * sum_x;

        // Handle edge case: all x values are the same (denominator = 0)
        if denominator.abs() < f64::EPSILON {
            return Err(anyhow::anyhow!(
                "Cannot compute regression: all timestamps are identical"
            ));
        }

        let slope = (n_f * sum_xy - sum_x * sum_y) / denominator;
        let intercept = (sum_y - slope * sum_x) / n_f;

        // Calculate R²
        let mean_y = sum_y / n_f;
        let ss_tot: f64 = y.iter().map(|yi| (yi - mean_y).powi(2)).sum();
        let ss_res: f64 = y
            .iter()
            .enumerate()
            .map(|(i, yi)| (yi - (intercept + slope * x[i])).powi(2))
            .sum();
        // For constant series (ss_tot = 0), if ss_res is also 0, it's a perfect fit (R² = 1.0)
        let r_squared = if ss_tot > 0.0 {
            1.0 - ss_res / ss_tot
        } else if ss_res.abs() < f64::EPSILON {
            1.0 // Perfect fit for constant series
        } else {
            0.0
        };

        // Standard error of the estimate
        let std_error = if n > 2 {
            (ss_res / (n_f - 2.0)).sqrt()
        } else {
            0.0
        };

        // Current value (most recent observation)
        let current_value = *y.last().unwrap_or(&0.0);

        // Calculate saturation threshold
        // If nominal_capacity is configured, use it as 100% reference
        // Otherwise fall back to max observed value with a warning flag
        let (saturation_threshold, capacity_source) =
            if let Some(nominal) = self.config.nominal_capacity {
                (
                    nominal * (self.config.saturation_threshold_pct / 100.0),
                    "nominal_capacity",
                )
            } else {
                let max_observed = y.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                (
                    max_observed * (self.config.saturation_threshold_pct / 100.0),
                    "max_observed_proxy",
                )
            };

        // Calculate time to saturation if slope > 0 and current_value < threshold
        let time_to_saturation = if slope > 0.0 && current_value < saturation_threshold {
            let seconds_to_saturation = (saturation_threshold - current_value) / slope;
            if seconds_to_saturation > 0.0 {
                Some(window_end + Duration::seconds(seconds_to_saturation as i64))
            } else {
                None
            }
        } else {
            None
        };

        // Determine if data is sufficient for reliable projection
        let insufficient_data_warning = n < 10 || r_squared < 0.3 || !is_ordered;

        // Determine resource type from metric
        let resource_type = if metric.contains("latency") {
            ResourceType::InterfaceUtilization
        } else {
            ResourceType::Custom(metric.to_string())
        };

        Ok(TrendAnalysis {
            resource_id: format!("{}:{}:{}", entity_id, metric, protocol),
            resource_type,
            entity_id: entity_id.to_string(),
            metric: metric.to_string(),
            protocol: protocol.to_string(),
            slope,
            intercept,
            r_squared,
            std_error,
            sample_count: n,
            window_start,
            window_end,
            time_to_saturation,
            current_value,
            saturation_threshold,
            confidence_level: self.config.confidence_level,
            analyzed_at: Utc::now(),
            insufficient_data_warning,
            capacity_source: capacity_source.to_string(),
        })
    }
}

impl CapacityReport {
    /// Generate a human-readable markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Capacity Planning Report\n\n");
        md.push_str(&format!(
            "**Generated:** {}\n",
            self.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        md.push_str(&format!(
            "**Analysis Period:** {} days\n",
            self.analysis_period_days
        ));
        md.push_str(&format!("**Trends Analyzed:** {}\n", self.trends.len()));
        md.push_str(&format!(
            "**Critical Resources:** {}\n",
            self.critical_resources.len()
        ));
        md.push_str(&format!(
            "**Insufficient Data:** {}\n\n",
            self.insufficient_data.len()
        ));

        if !self.critical_resources.is_empty() {
            md.push_str("## Critical Resources (Time to Saturation)\n\n");
            md.push_str("| Resource | Entity | Current Value | Saturation Threshold | Capacity Source | Time to Saturation | R² |\n");
            md.push_str("|----------|--------|---------------|---------------------|-----------------|-------------------|----|\n");
            for trend in &self.critical_resources {
                let time_to_sat = trend.time_to_saturation.map_or("N/A".to_string(), |t| {
                    t.format("%Y-%m-%d %H:%M:%S UTC").to_string()
                });
                md.push_str(&format!(
                    "| {} | {} | {:.2} | {:.2} | {} | {} | {:.3} |\n",
                    trend.metric,
                    trend.entity_id,
                    trend.current_value,
                    trend.saturation_threshold,
                    trend.capacity_source,
                    time_to_sat,
                    trend.r_squared
                ));
            }
            md.push('\n');
        }

        if !self.trends.is_empty() {
            md.push_str("## All Trends\n\n");
            md.push_str(
                "| Resource | Entity | Slope | R² | Samples | Capacity Source | Warning |\n",
            );
            md.push_str(
                "|----------|--------|-------|-----|---------|-----------------|--------|\n",
            );
            for trend in &self.trends {
                let warning = if trend.insufficient_data_warning {
                    "⚠️ Insufficient data"
                } else {
                    ""
                };
                md.push_str(&format!(
                    "| {} | {} | {:.6} | {:.3} | {} | {} | {} |\n",
                    trend.metric,
                    trend.entity_id,
                    trend.slope,
                    trend.r_squared,
                    trend.sample_count,
                    trend.capacity_source,
                    warning
                ));
            }
            md.push('\n');
        }

        if !self.insufficient_data.is_empty() {
            md.push_str("## Resources with Insufficient Data\n\n");
            for item in &self.insufficient_data {
                md.push_str(&format!("- {}\n", item));
            }
            md.push('\n');
        }

        md.push_str("## Configuration\n\n");
        md.push_str(&format!(
            "- **Lookback Window:** {} seconds\n",
            self.config_used.lookback_window_seconds
        ));
        md.push_str(&format!(
            "- **Min Samples:** {}\n",
            self.config_used.min_samples
        ));
        md.push_str(&format!(
            "- **Saturation Threshold:** {}%\n",
            self.config_used.saturation_threshold_pct
        ));
        md.push_str(&format!(
            "- **Confidence Level:** {}\n",
            self.config_used.confidence_level
        ));
        md.push_str(&format!(
            "- **Nominal Capacity:** {}\n",
            self.config_used.nominal_capacity.map_or(
                "not set (using max observed proxy)".to_string(),
                |v| format!("{}", v)
            )
        ));
        md.push_str(&format!("- **Enabled:** {}\n", self.config_used.enabled));

        md.push_str("\n---\n");
        md.push_str("*This report is generated automatically. Projections are based on linear regression and should be validated with domain expertise.*\n");
        md.push_str("*Note: When nominal capacity is not configured, saturation threshold is calculated as a percentage of the maximum observed value, which may underestimate true capacity.*\n");

        md
    }
}

impl CapacityReport {
    /// Generate JSON report
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
