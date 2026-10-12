use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, FromRow, Row, SqlitePool};
use std::path::PathBuf;

pub struct Storage {
    pub pool: SqlitePool,
}

impl Storage {
    pub async fn connect(path: &str) -> Result<Self> {
        let file = PathBuf::from(path);
        if !file.exists() {
            std::fs::create_dir_all(file.parent().unwrap()).with_context(|| "create data dir")?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(opts)
            .await?;

        // Run migrations from the migrations directory
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations"))
            .await
            .context("creating migrator")?;
        migrator.run(&pool).await.context("running migrations")?;

        Ok(Storage { pool })
    }

    /// Persist a health check observation
    pub async fn insert_observation(&self, obs: &Observation) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO observations (id, device_id, metric, value, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&obs.id)
        .bind(&obs.device_id)
        .bind(&obs.metric)
        .bind(obs.value)
        .bind(&obs.source)
        .bind(&obs.confidence)
        .bind(obs.observed_at.to_rfc3339())
        .bind(obs.created_at.to_rfc3339())
        .bind(obs.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get observations for a device/metric within a time window
    pub async fn get_observations(
        &self,
        device_id: &str,
        metric: &str,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
        limit: Option<i64>,
    ) -> Result<Vec<Observation>> {
        let mut query = String::from(
            "SELECT * FROM observations WHERE device_id = ? AND metric = ? AND observed_at >= ?",
        );
        let mut args: Vec<String> = vec![
            device_id.to_string(),
            metric.to_string(),
            since.to_rfc3339(),
        ];

        if let Some(until) = until {
            query.push_str(" AND observed_at <= ?");
            args.push(until.to_rfc3339());
        }
        query.push_str(" ORDER BY observed_at DESC");
        if let Some(limit) = limit {
            query.push_str(" LIMIT ?");
            args.push(limit.to_string());
        }

        let mut q = sqlx::query_as::<_, ObservationRow>(&query);
        for arg in args {
            q = q.bind(arg);
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(|r| r.into_observation()).collect()
    }

    /// Get latency observations for percentile computation
    pub async fn get_latency_observations(
        &self,
        device_id: &str,
        protocol: &str,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<f64>> {
        let metric = format!("{}_latency_ms", protocol);
        let mut query = String::from(
            "SELECT value FROM observations WHERE device_id = ? AND metric = ? AND observed_at >= ? AND value > 0"
        );
        let mut args: Vec<String> = vec![device_id.to_string(), metric, since.to_rfc3339()];

        if let Some(until) = until {
            query.push_str(" AND observed_at <= ?");
            args.push(until.to_rfc3339());
        }
        query.push_str(" ORDER BY observed_at DESC");

        let mut q = sqlx::query(&query);
        for arg in args {
            q = q.bind(arg);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| r.get::<f64, _>("value")).collect())
    }

    /// Compute percentile from observations (uses SQLite's percentile approximation via ORDER BY + LIMIT)
    pub async fn compute_percentile(
        &self,
        device_id: &str,
        metric: &str,
        percentile: f64,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Option<f64>> {
        let values = self
            .get_latency_observations(device_id, metric, since, until)
            .await?;
        if values.is_empty() {
            return Ok(None);
        }
        let mut sorted = values;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((percentile / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        Ok(Some(sorted[idx.min(sorted.len() - 1)]))
    }

    /// Get availability stats for a device/protocol in a time window
    pub async fn get_availability_stats(
        &self,
        device_id: &str,
        protocol: &str,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<AvailabilityStats> {
        let metric = format!("{}_latency_ms", protocol);
        let mut query = String::from(
            "SELECT COUNT(*) as total, SUM(CASE WHEN value > 0 THEN 1 ELSE 0 END) as successful FROM observations WHERE device_id = ? AND metric = ? AND observed_at >= ?"
        );
        let mut args: Vec<String> = vec![device_id.to_string(), metric, since.to_rfc3339()];

        if let Some(until) = until {
            query.push_str(" AND observed_at <= ?");
            args.push(until.to_rfc3339());
        }

        let mut q = sqlx::query(&query);
        for arg in args {
            q = q.bind(arg);
        }
        let row = q.fetch_one(&self.pool).await?;
        let total: i64 = row.get("total");
        let successful: i64 = row.get("successful");

        Ok(AvailabilityStats {
            total_checks: total as u64,
            successful_checks: successful as u64,
            availability_pct: if total > 0 {
                (successful as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// Get observation count for a device/metric
    pub async fn get_observation_count(
        &self,
        device_id: &str,
        metric: &str,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<u64> {
        let mut query = String::from(
            "SELECT COUNT(*) as count FROM observations WHERE device_id = ? AND metric = ? AND observed_at >= ?"
        );
        let mut args: Vec<String> = vec![
            device_id.to_string(),
            metric.to_string(),
            since.to_rfc3339(),
        ];

        if let Some(until) = until {
            query.push_str(" AND observed_at <= ?");
            args.push(until.to_rfc3339());
        }

        let mut q = sqlx::query(&query);
        for arg in args {
            q = q.bind(arg);
        }
        let row = q.fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>("count") as u64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: String,
    pub device_id: String,
    pub metric: String,
    pub value: f64,
    pub source: String,
    pub confidence: String,
    pub observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct ObservationRow {
    id: String,
    device_id: String,
    metric: String,
    value: f64,
    source: String,
    confidence: String,
    observed_at: String,
    created_at: String,
    updated_at: String,
}

impl ObservationRow {
    fn into_observation(self) -> Result<Observation> {
        Ok(Observation {
            id: self.id,
            device_id: self.device_id,
            metric: self.metric,
            value: self.value,
            source: self.source,
            confidence: self.confidence,
            observed_at: self.observed_at.parse()?,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailabilityStats {
    pub total_checks: u64,
    pub successful_checks: u64,
    pub availability_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PercentileResult {
    pub percentile: f64,
    pub value: f64,
    pub sample_count: usize,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
}
