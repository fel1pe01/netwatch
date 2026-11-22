use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, FromRow, Row, SqlitePool};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct Storage {
    pub pool: SqlitePool,
}

impl Storage {
    /// Connect with default busy_timeout (5000ms)
    pub async fn connect(path: &str) -> Result<Self> {
        Self::connect_with_options(path, 5000).await
    }

    /// Connect with custom busy_timeout in milliseconds
    pub async fn connect_with_options(path: &str, busy_timeout_ms: u32) -> Result<Self> {
        let file = PathBuf::from(path);
        if !file.exists() {
            std::fs::create_dir_all(file.parent().unwrap()).with_context(|| "create data dir")?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_millis(busy_timeout_ms as u64));
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(opts)
            .await?;

        // Run migrations from the migrations directory
        let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("./migrations"))
            .await
            .context("creating migrator")?;
        migrator.run(&pool).await.context("running migrations")?;

        let storage = Storage { pool };
        // Ensure the local device exists for legacy health check jobs
        storage.ensure_local_device().await?;

        Ok(storage)
    }

    /// Ensure the "local" device exists for legacy health check jobs
    async fn ensure_local_device(&self) -> Result<()> {
        use sqlx::query;
        let now = Utc::now().to_rfc3339();
        query(
            r#"
            INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                hostname = excluded.hostname,
                ip = excluded.ip,
                updated_at = excluded.updated_at
            "#,
        )
        .bind("local")
        .bind("local")
        .bind("127.0.0.1")
        .bind("")
        .bind("loopback")
        .bind("system")
        .bind("high")
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(())
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

    /// Get latency observations with timestamps for time series analysis
    pub async fn get_latency_observations_with_timestamps(
        &self,
        device_id: &str,
        protocol: &str,
        since: DateTime<Utc>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Vec<Observation>> {
        let metric = format!("{}_latency_ms", protocol);
        let mut query = String::from(
            "SELECT id, device_id, metric, value, source, confidence, observed_at, created_at, updated_at FROM observations WHERE device_id = ? AND metric = ? AND observed_at >= ? AND value > 0"
        );
        let mut args: Vec<String> = vec![device_id.to_string(), metric, since.to_rfc3339()];

        if let Some(until) = until {
            query.push_str(" AND observed_at <= ?");
            args.push(until.to_rfc3339());
        }
        query.push_str(" ORDER BY observed_at ASC");

        let mut q = sqlx::query(&query);
        for arg in args {
            q = q.bind(arg);
        }
        let rows = q.fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                ObservationRow {
                    id: r.get("id"),
                    device_id: r.get("device_id"),
                    metric: r.get("metric"),
                    value: r.get("value"),
                    source: r.get("source"),
                    confidence: r.get("confidence"),
                    observed_at: r.get("observed_at"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                }
                .into_observation()
                .unwrap()
            })
            .collect())
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

/// Check result for persistence (single-writer actor pattern)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub target: String,
    pub protocol: String,
    pub success: bool,
    pub latency_ms: Option<u128>,
    pub error: Option<String>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_state_change: bool,
    pub previous_state: Option<bool>,
    pub observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Event v2 for persistence (matching Event struct)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventV2 {
    pub id: String,
    pub schema_version: u32,
    pub event_type: String,
    pub event_ts: DateTime<Utc>,
    pub observed_ts: DateTime<Utc>,
    pub entity_id: String,
    pub entity_type: String,
    pub severity: String,
    pub state: String,
    pub source: String,
    pub evidence: String, // JSON
    pub dedup_key: String,
    pub correlation_id: Option<String>,
    pub trace_id: Option<String>,
    pub metadata: String, // JSON
    pub created_at: DateTime<Utc>,
}

/// Upsert interface message for writer actor
#[derive(Debug, Clone)]
pub struct UpsertInterfaceMessage {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub mac: Option<String>,
    pub mtu: Option<i64>,
    pub state: Option<String>,
    pub source: String,
    pub confidence: String,
    pub observed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_seen: String,
}

/// Upsert route message for writer actor
#[derive(Debug, Clone)]
pub struct UpsertRouteMessage {
    pub id: String,
    pub destination: String,
    pub gateway: Option<String>,
    pub interface: String,
    pub metric: Option<i64>,
    pub mask: Option<String>,
    pub family: String,
    pub last_seen: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Message for the check result writer actor (extended for inventory)
#[derive(Debug)]
pub enum CheckResultMessage {
    WriteCheckResult(CheckResult),
    WriteEvent(EventV2),
    UpsertInterface(UpsertInterfaceMessage),
    UpsertRoute(UpsertRouteMessage),
    Shutdown,
}

/// Check result writer actor - single writer for check_results and events_v2 tables
pub struct CheckResultWriter {
    pool: SqlitePool,
    rx: mpsc::Receiver<CheckResultMessage>,
}

impl CheckResultWriter {
    pub fn new(pool: SqlitePool, buffer: usize) -> (Self, mpsc::Sender<CheckResultMessage>) {
        let (tx, rx) = mpsc::channel(buffer);
        let writer = Self { pool, rx };
        (writer, tx)
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                CheckResultMessage::WriteCheckResult(cr) => {
                    if let Err(e) = self.write_check_result(&cr).await {
                        tracing::error!("Failed to write check result: {}", e);
                    }
                }
                CheckResultMessage::WriteEvent(ev) => {
                    if let Err(e) = self.write_event(&ev).await {
                        tracing::error!("Failed to write event: {}", e);
                    }
                }
                CheckResultMessage::UpsertInterface(msg) => {
                    if let Err(e) = self.upsert_interface(&msg).await {
                        tracing::error!("Failed to upsert interface: {}", e);
                    }
                }
                CheckResultMessage::UpsertRoute(msg) => {
                    if let Err(e) = self.upsert_route(&msg).await {
                        tracing::error!("Failed to upsert route: {}", e);
                    }
                }
                CheckResultMessage::Shutdown => {
                    tracing::info!("CheckResultWriter shutting down");
                    break;
                }
            }
        }
    }

    async fn write_check_result(&self, cr: &CheckResult) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO check_results (
                id, entity_id, entity_type, target, protocol, success, latency_ms,
                error, consecutive_failures, consecutive_successes, last_state_change,
                previous_state, observed_at, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&cr.id)
        .bind(&cr.entity_id)
        .bind(&cr.entity_type)
        .bind(&cr.target)
        .bind(&cr.protocol)
        .bind(cr.success)
        .bind(cr.latency_ms.map(|v| v as i64))
        .bind(&cr.error)
        .bind(cr.consecutive_failures as i64)
        .bind(cr.consecutive_successes as i64)
        .bind(cr.last_state_change)
        .bind(cr.previous_state)
        .bind(cr.observed_at.to_rfc3339())
        .bind(cr.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn write_event(&self, ev: &EventV2) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO events_v2 (
                id, schema_version, event_type, event_ts, observed_ts,
                entity_id, entity_type, severity, state, source,
                evidence, dedup_key, correlation_id, trace_id, metadata, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&ev.id)
        .bind(ev.schema_version as i64)
        .bind(&ev.event_type)
        .bind(ev.event_ts.to_rfc3339())
        .bind(ev.observed_ts.to_rfc3339())
        .bind(&ev.entity_id)
        .bind(&ev.entity_type)
        .bind(&ev.severity)
        .bind(&ev.state)
        .bind(&ev.source)
        .bind(&ev.evidence)
        .bind(&ev.dedup_key)
        .bind(&ev.correlation_id)
        .bind(&ev.trace_id)
        .bind(&ev.metadata)
        .bind(ev.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_interface(&self, msg: &UpsertInterfaceMessage) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO interfaces (
                id, device_id, name, mac, mtu, state, source, confidence, observed_at, created_at, updated_at, last_seen
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                device_id = excluded.device_id,
                name = excluded.name,
                mac = excluded.mac,
                mtu = excluded.mtu,
                state = excluded.state,
                source = excluded.source,
                confidence = excluded.confidence,
                observed_at = excluded.observed_at,
                updated_at = excluded.updated_at,
                last_seen = excluded.last_seen
            "#,
        )
        .bind(&msg.id)
        .bind(&msg.device_id)
        .bind(&msg.name)
        .bind(&msg.mac)
        .bind(msg.mtu)
        .bind(&msg.state)
        .bind(&msg.source)
        .bind(&msg.confidence)
        .bind(&msg.observed_at)
        .bind(&msg.created_at)
        .bind(&msg.updated_at)
        .bind(&msg.last_seen)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_route(&self, msg: &UpsertRouteMessage) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO routes (
                id, destination, gateway, interface, metric, mask, family, last_seen, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                destination = excluded.destination,
                gateway = excluded.gateway,
                interface = excluded.interface,
                metric = excluded.metric,
                mask = excluded.mask,
                family = excluded.family,
                last_seen = excluded.last_seen,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&msg.id)
        .bind(&msg.destination)
        .bind(&msg.gateway)
        .bind(&msg.interface)
        .bind(msg.metric)
        .bind(&msg.mask)
        .bind(&msg.family)
        .bind(&msg.last_seen)
        .bind(&msg.created_at)
        .bind(&msg.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

/// Load last known state for an entity from check_results
pub async fn load_last_check_state(
    pool: &SqlitePool,
    entity_id: &str,
    entity_type: &str,
) -> Result<Option<(bool, u32, u32)>> {
    // Returns (last_success, consecutive_failures, consecutive_successes)
    let row = sqlx::query(
        r#"
        SELECT success, consecutive_failures, consecutive_successes
        FROM check_results
        WHERE entity_id = ? AND entity_type = ?
        ORDER BY observed_at DESC
        LIMIT 1
        "#,
    )
    .bind(entity_id)
    .bind(entity_type)
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let success: bool = row.get("success");
        let consecutive_failures: i64 = row.get("consecutive_failures");
        let consecutive_successes: i64 = row.get("consecutive_successes");
        Ok(Some((
            success,
            consecutive_failures as u32,
            consecutive_successes as u32,
        )))
    } else {
        Ok(None)
    }
}

/// Prune old check_results and events_v2 based on retention config
pub async fn prune_old_data(
    pool: &SqlitePool,
    check_results_retention_days: u32,
    events_retention_days: u32,
) -> Result<(u64, u64)> {
    let check_cutoff = Utc::now() - chrono::Duration::days(check_results_retention_days as i64);
    let events_cutoff = Utc::now() - chrono::Duration::days(events_retention_days as i64);

    let check_deleted = sqlx::query("DELETE FROM check_results WHERE observed_at < ?")
        .bind(check_cutoff.to_rfc3339())
        .execute(pool)
        .await?
        .rows_affected();

    let events_deleted = sqlx::query("DELETE FROM events_v2 WHERE event_ts < ?")
        .bind(events_cutoff.to_rfc3339())
        .execute(pool)
        .await?
        .rows_affected();

    Ok((check_deleted, events_deleted))
}

/// Prune absent interfaces and routes based on retention config
/// Deletes interfaces and routes where last_seen is older than retention_days
pub async fn prune_absent_inventory(pool: &SqlitePool, retention_days: u32) -> Result<(u64, u64)> {
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_str = cutoff.to_rfc3339();

    let interfaces_deleted =
        sqlx::query("DELETE FROM interfaces WHERE last_seen IS NOT NULL AND last_seen < ?")
            .bind(&cutoff_str)
            .execute(pool)
            .await?
            .rows_affected();

    let routes_deleted =
        sqlx::query("DELETE FROM routes WHERE last_seen IS NOT NULL AND last_seen < ?")
            .bind(&cutoff_str)
            .execute(pool)
            .await?
            .rows_affected();

    Ok((interfaces_deleted, routes_deleted))
}

/// Filter parameters for check results query
#[derive(Default)]
pub struct CheckResultsFilter {
    pub entity_id: Option<String>,
    pub entity_type: Option<String>,
    pub protocol: Option<String>,
    pub success: Option<bool>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

/// Get check results with pagination
pub async fn get_check_results(
    pool: &SqlitePool,
    filter: CheckResultsFilter,
) -> Result<PaginatedResponse<CheckResult>> {
    let mut where_clauses = Vec::new();
    let mut args: Vec<String> = vec![];

    if let Some(eid) = filter.entity_id {
        where_clauses.push("entity_id = ?".to_string());
        args.push(eid);
    }
    if let Some(etype) = filter.entity_type {
        where_clauses.push("entity_type = ?".to_string());
        args.push(etype);
    }
    if let Some(proto) = filter.protocol {
        where_clauses.push("protocol = ?".to_string());
        args.push(proto);
    }
    if let Some(s) = filter.success {
        where_clauses.push("success = ?".to_string());
        args.push(s.to_string());
    }
    if let Some(s) = filter.since {
        where_clauses.push("observed_at >= ?".to_string());
        args.push(s.to_rfc3339());
    }
    if let Some(u) = filter.until {
        where_clauses.push("observed_at <= ?".to_string());
        args.push(u.to_rfc3339());
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM check_results {}", where_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for arg in &args {
        count_q = count_q.bind(arg);
    }
    let total: i64 = count_q.fetch_one(pool).await.unwrap_or(0);

    // Get paginated results
    let limit = filter.limit.clamp(1, 500);
    let offset = filter.offset.max(0);

    let items_query = format!(
        "SELECT * FROM check_results {} ORDER BY observed_at DESC LIMIT ? OFFSET ?",
        where_sql
    );
    let mut items_q = sqlx::query_as::<_, CheckResultRow>(&items_query);
    for arg in &args {
        items_q = items_q.bind(arg);
    }
    items_q = items_q.bind(limit).bind(offset);
    let rows = items_q.fetch_all(pool).await?;
    let items = rows
        .into_iter()
        .map(|r| r.into_check_result())
        .collect::<Result<Vec<_>>>()?;

    Ok(PaginatedResponse {
        items,
        total,
        limit,
        offset,
    })
}

/// Filter parameters for events query
#[derive(Default)]
pub struct EventsFilter {
    pub entity_id: Option<String>,
    pub entity_type: Option<String>,
    pub event_type: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

/// Get events with pagination
pub async fn get_events(
    pool: &SqlitePool,
    filter: EventsFilter,
) -> Result<PaginatedResponse<EventV2>> {
    let mut where_clauses = Vec::new();
    let mut args: Vec<String> = vec![];

    if let Some(eid) = filter.entity_id {
        where_clauses.push("entity_id = ?".to_string());
        args.push(eid);
    }
    if let Some(etype) = filter.entity_type {
        where_clauses.push("entity_type = ?".to_string());
        args.push(etype);
    }
    if let Some(etype) = filter.event_type {
        where_clauses.push("event_type = ?".to_string());
        args.push(etype);
    }
    if let Some(s) = filter.since {
        where_clauses.push("event_ts >= ?".to_string());
        args.push(s.to_rfc3339());
    }
    if let Some(u) = filter.until {
        where_clauses.push("event_ts <= ?".to_string());
        args.push(u.to_rfc3339());
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM events_v2 {}", where_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for arg in &args {
        count_q = count_q.bind(arg);
    }
    let total: i64 = count_q.fetch_one(pool).await.unwrap_or(0);

    // Get paginated results
    let limit = filter.limit.clamp(1, 500);
    let offset = filter.offset.max(0);

    let items_query = format!(
        "SELECT * FROM events_v2 {} ORDER BY event_ts DESC LIMIT ? OFFSET ?",
        where_sql
    );
    let mut items_q = sqlx::query_as::<_, EventV2Row>(&items_query);
    for arg in &args {
        items_q = items_q.bind(arg);
    }
    items_q = items_q.bind(limit).bind(offset);
    let rows = items_q.fetch_all(pool).await?;
    let items = rows
        .into_iter()
        .map(|r| r.into_event())
        .collect::<Result<Vec<_>>>()?;

    Ok(PaginatedResponse {
        items,
        total,
        limit,
        offset,
    })
}

/// Pagination response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

/// Filter parameters for devices query
#[derive(Default)]
pub struct DevicesFilter {
    pub id: Option<String>,
    pub ip: Option<String>,
    pub hostname: Option<String>,
    pub device_type: Option<String>,
    pub limit: i64,
    pub offset: i64,
    pub order_by: Option<String>,
    pub order_dir: Option<String>,
}

/// Get devices with pagination from DB
pub async fn get_devices(
    pool: &SqlitePool,
    filter: DevicesFilter,
) -> Result<PaginatedResponse<DeviceRow>> {
    // Whitelist allowed ORDER BY columns
    let allowed_order_by = [
        "id",
        "hostname",
        "ip",
        "device_type",
        "created_at",
        "updated_at",
    ];
    let order_by = filter
        .order_by
        .as_deref()
        .filter(|c| allowed_order_by.contains(c))
        .unwrap_or("created_at");
    let order_dir = filter
        .order_dir
        .as_deref()
        .filter(|d| d.eq_ignore_ascii_case("asc") || d.eq_ignore_ascii_case("desc"))
        .unwrap_or("DESC");

    // Build WHERE clause for exact-match filters only
    let mut where_clauses: Vec<String> = vec![];
    let mut args: Vec<String> = vec![];

    if let Some(id) = filter.id {
        where_clauses.push("id = ?".to_string());
        args.push(id);
    }
    if let Some(ip) = filter.ip {
        where_clauses.push("ip = ?".to_string());
        args.push(ip);
    }
    if let Some(hostname) = filter.hostname {
        where_clauses.push("hostname = ?".to_string());
        args.push(hostname);
    }
    if let Some(device_type) = filter.device_type {
        where_clauses.push("device_type = ?".to_string());
        args.push(device_type);
    }

    let where_sql = if where_clauses.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Count total
    let count_query = format!("SELECT COUNT(*) FROM devices {}", where_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for arg in &args {
        count_q = count_q.bind(arg);
    }
    let total: i64 = count_q.fetch_one(pool).await.unwrap_or(0);

    // Validate and clamp limit/offset
    let limit = filter.limit.clamp(1, 500);
    let offset = filter.offset.max(0);

    // Fetch items with explicit columns (no SELECT *)
    let items_query = format!(
        "SELECT id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at \
         FROM devices {} ORDER BY {} {} LIMIT ? OFFSET ?",
        where_sql, order_by, order_dir
    );
    let mut items_q = sqlx::query_as::<_, DeviceRow>(&items_query);
    for arg in &args {
        items_q = items_q.bind(arg);
    }
    items_q = items_q.bind(limit).bind(offset);
    let items = items_q.fetch_all(pool).await?;

    // Devices don't have last_seen, so they are always present
    let items = items
        .into_iter()
        .map(|mut r| {
            r.present = true;
            r
        })
        .collect();

    Ok(PaginatedResponse {
        items,
        total,
        limit,
        offset,
    })
}

/// Device row from DB
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeviceRow {
    pub id: String,
    pub hostname: Option<String>,
    pub ip: Option<String>,
    pub mac: Option<String>,
    pub device_type: Option<String>,
    pub source: Option<String>,
    pub confidence: Option<String>,
    pub observed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Whether the device is considered present (not absent)
    #[sqlx(default)]
    pub present: bool,
}

/// Filter parameters for interfaces query
#[derive(Default)]
pub struct InterfacesFilter {
    pub device_id: Option<String>,
    pub name: Option<String>,
    pub present: Option<bool>,
    pub limit: i64,
    pub offset: i64,
    pub order_by: Option<String>,
    pub order_dir: Option<String>,
}

/// Get interfaces with pagination from DB
pub async fn get_interfaces(
    pool: &SqlitePool,
    filter: InterfacesFilter,
    absence_threshold_seconds: i64,
) -> Result<PaginatedResponse<InterfaceRow>> {
    let allowed_order_by = [
        "id",
        "device_id",
        "name",
        "mac",
        "mtu",
        "state",
        "created_at",
        "updated_at",
    ];
    let order_by = filter
        .order_by
        .as_deref()
        .filter(|c| allowed_order_by.contains(c))
        .unwrap_or("created_at");
    let order_dir = filter
        .order_dir
        .as_deref()
        .filter(|d| d.eq_ignore_ascii_case("asc") || d.eq_ignore_ascii_case("desc"))
        .unwrap_or("DESC");

    let mut where_clauses: Vec<String> = vec![];
    let mut args: Vec<String> = vec![];

    if let Some(device_id) = filter.device_id {
        where_clauses.push("device_id = ?".to_string());
        args.push(device_id);
    }
    if let Some(name) = filter.name {
        where_clauses.push("name = ?".to_string());
        args.push(name);
    }
    // Add present filter
    if let Some(present) = filter.present {
        let threshold = Utc::now() - chrono::Duration::seconds(absence_threshold_seconds);
        let threshold_str = threshold.to_rfc3339();
        if present {
            // present = true: last_seen > (now - threshold)
            where_clauses.push("(last_seen IS NOT NULL AND last_seen > ?)".to_string());
            args.push(threshold_str);
        } else {
            // present = false: last_seen IS NULL OR last_seen <= (now - threshold)
            where_clauses.push("(last_seen IS NULL OR last_seen <= ?)".to_string());
            args.push(threshold_str);
        }
    }

    let where_sql = if where_clauses.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM interfaces {}", where_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for arg in &args {
        count_q = count_q.bind(arg);
    }
    let total: i64 = count_q.fetch_one(pool).await.unwrap_or(0);

    let limit = filter.limit.clamp(1, 500);
    let offset = filter.offset.max(0);

    let items_query = format!(
        "SELECT id, device_id, name, mac, mtu, state, source, confidence, observed_at, created_at, updated_at, last_seen \
         FROM interfaces {} ORDER BY {} {} LIMIT ? OFFSET ?",
        where_sql, order_by, order_dir
    );
    let mut items_q = sqlx::query_as::<_, InterfaceRow>(&items_query);
    for arg in &args {
        items_q = items_q.bind(arg);
    }
    items_q = items_q.bind(limit).bind(offset);
    let items = items_q.fetch_all(pool).await?;

    // Compute present field based on last_seen and absence threshold
    let threshold = Utc::now() - chrono::Duration::seconds(absence_threshold_seconds);
    let items = items
        .into_iter()
        .map(|mut r| {
            r.present = r
                .last_seen
                .as_ref()
                .and_then(|ls| ls.parse::<DateTime<Utc>>().ok())
                .map(|last_seen| last_seen > threshold)
                .unwrap_or(false);
            r
        })
        .collect();

    Ok(PaginatedResponse {
        items,
        total,
        limit,
        offset,
    })
}

/// Interface row from DB
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InterfaceRow {
    pub id: String,
    pub device_id: String,
    pub name: Option<String>,
    pub mac: Option<String>,
    pub mtu: Option<i64>,
    pub state: Option<String>,
    pub source: Option<String>,
    pub confidence: Option<String>,
    pub observed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_seen: Option<String>,
    /// Whether the interface is considered present (not absent)
    #[sqlx(default)]
    pub present: bool,
}

/// Filter parameters for routes query
#[derive(Default)]
pub struct RoutesFilter {
    pub destination: Option<String>,
    pub interface: Option<String>,
    pub gateway: Option<String>,
    pub present: Option<bool>,
    pub limit: i64,
    pub offset: i64,
    pub order_by: Option<String>,
    pub order_dir: Option<String>,
}

/// Get routes with pagination from DB
pub async fn get_routes(
    pool: &SqlitePool,
    filter: RoutesFilter,
    absence_threshold_seconds: i64,
) -> Result<PaginatedResponse<RouteRow>> {
    let allowed_order_by = [
        "id",
        "destination",
        "gateway",
        "interface",
        "metric",
        "family",
        "created_at",
        "updated_at",
    ];
    let order_by = filter
        .order_by
        .as_deref()
        .filter(|c| allowed_order_by.contains(c))
        .unwrap_or("created_at");
    let order_dir = filter
        .order_dir
        .as_deref()
        .filter(|d| d.eq_ignore_ascii_case("asc") || d.eq_ignore_ascii_case("desc"))
        .unwrap_or("DESC");

    let mut where_clauses: Vec<String> = vec![];
    let mut args: Vec<String> = vec![];

    if let Some(destination) = filter.destination {
        where_clauses.push("destination = ?".to_string());
        args.push(destination);
    }
    if let Some(interface) = filter.interface {
        where_clauses.push("interface = ?".to_string());
        args.push(interface);
    }
    if let Some(gateway) = filter.gateway {
        where_clauses.push("gateway = ?".to_string());
        args.push(gateway);
    }
    // Add present filter
    if let Some(present) = filter.present {
        let threshold = Utc::now() - chrono::Duration::seconds(absence_threshold_seconds);
        let threshold_str = threshold.to_rfc3339();
        if present {
            where_clauses.push("(last_seen IS NOT NULL AND last_seen > ?)".to_string());
            args.push(threshold_str);
        } else {
            where_clauses.push("(last_seen IS NULL OR last_seen <= ?)".to_string());
            args.push(threshold_str);
        }
    }

    let where_sql = if where_clauses.is_empty() {
        "".to_string()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM routes {}", where_sql);
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for arg in &args {
        count_q = count_q.bind(arg);
    }
    let total: i64 = count_q.fetch_one(pool).await.unwrap_or(0);

    let limit = filter.limit.clamp(1, 500);
    let offset = filter.offset.max(0);

    let items_query = format!(
        "SELECT id, destination, gateway, interface, metric, mask, family, last_seen, created_at, updated_at \
         FROM routes {} ORDER BY {} {} LIMIT ? OFFSET ?",
        where_sql, order_by, order_dir
    );
    let mut items_q = sqlx::query_as::<_, RouteRow>(&items_query);
    for arg in &args {
        items_q = items_q.bind(arg);
    }
    items_q = items_q.bind(limit).bind(offset);
    let items = items_q.fetch_all(pool).await?;

    // Compute present field based on last_seen and absence threshold
    let threshold = Utc::now() - chrono::Duration::seconds(absence_threshold_seconds);
    let items = items
        .into_iter()
        .map(|mut r| {
            r.present = r
                .last_seen
                .parse::<DateTime<Utc>>()
                .map(|last_seen| last_seen > threshold)
                .unwrap_or(false);
            r
        })
        .collect();

    Ok(PaginatedResponse {
        items,
        total,
        limit,
        offset,
    })
}

/// Route row from DB
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct RouteRow {
    pub id: String,
    pub destination: String,
    pub gateway: Option<String>,
    pub interface: String,
    pub metric: Option<i64>,
    pub mask: Option<String>,
    pub family: String,
    pub last_seen: String,
    pub created_at: String,
    pub updated_at: String,
    /// Whether the route is considered present (not absent)
    #[sqlx(default)]
    pub present: bool,
}

#[derive(FromRow)]
struct CheckResultRow {
    id: String,
    entity_id: String,
    entity_type: String,
    target: String,
    protocol: String,
    success: bool,
    latency_ms: Option<i64>,
    error: Option<String>,
    consecutive_failures: i64,
    consecutive_successes: i64,
    last_state_change: bool,
    previous_state: Option<bool>,
    observed_at: String,
    created_at: String,
}

impl CheckResultRow {
    fn into_check_result(self) -> Result<CheckResult> {
        Ok(CheckResult {
            id: self.id,
            entity_id: self.entity_id,
            entity_type: self.entity_type,
            target: self.target,
            protocol: self.protocol,
            success: self.success,
            latency_ms: self.latency_ms.map(|v| v as u128),
            error: self.error,
            consecutive_failures: self.consecutive_failures as u32,
            consecutive_successes: self.consecutive_successes as u32,
            last_state_change: self.last_state_change,
            previous_state: self.previous_state,
            observed_at: self.observed_at.parse()?,
            created_at: self.created_at.parse()?,
        })
    }
}

#[derive(FromRow)]
struct EventV2Row {
    id: String,
    schema_version: i64,
    event_type: String,
    event_ts: String,
    observed_ts: String,
    entity_id: String,
    entity_type: String,
    severity: String,
    state: String,
    source: String,
    evidence: String,
    dedup_key: String,
    correlation_id: Option<String>,
    trace_id: Option<String>,
    metadata: String,
    created_at: String,
}

impl EventV2Row {
    fn into_event(self) -> Result<EventV2> {
        Ok(EventV2 {
            id: self.id,
            schema_version: self.schema_version as u32,
            event_type: self.event_type,
            event_ts: self.event_ts.parse()?,
            observed_ts: self.observed_ts.parse()?,
            entity_id: self.entity_id,
            entity_type: self.entity_type,
            severity: self.severity,
            state: self.state,
            source: self.source,
            evidence: self.evidence,
            dedup_key: self.dedup_key,
            correlation_id: self.correlation_id,
            trace_id: self.trace_id,
            metadata: self.metadata,
            created_at: self.created_at.parse()?,
        })
    }
}
