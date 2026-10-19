use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, SqlitePool};
use std::collections::HashMap;

/// Service status enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Unknown,
    Up,
    Down,
    Degraded,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceStatus::Unknown => write!(f, "unknown"),
            ServiceStatus::Up => write!(f, "up"),
            ServiceStatus::Down => write!(f, "down"),
            ServiceStatus::Degraded => write!(f, "degraded"),
        }
    }
}

impl std::str::FromStr for ServiceStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unknown" => Ok(ServiceStatus::Unknown),
            "up" => Ok(ServiceStatus::Up),
            "down" => Ok(ServiceStatus::Down),
            "degraded" => Ok(ServiceStatus::Degraded),
            _ => Ok(ServiceStatus::Unknown),
        }
    }
}

/// Service model for tracking service status independently from host
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub device_id: String,
    pub port: i64,
    pub protocol: String,
    pub name: Option<String>,
    pub status: ServiceStatus,
    pub source: Option<String>,
    pub confidence: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Status tracking fields
    pub last_check_at: Option<DateTime<Utc>>,
    pub last_check_success: Option<bool>,
    pub last_check_latency_ms: Option<i64>,
    pub check_interval: i64,
}

#[derive(FromRow)]
struct ServiceRow {
    id: String,
    device_id: String,
    port: i64,
    protocol: String,
    name: Option<String>,
    status: Option<String>,
    source: Option<String>,
    confidence: Option<String>,
    observed_at: Option<String>,
    created_at: String,
    updated_at: String,
    last_check_at: Option<String>,
    last_check_success: Option<bool>,
    last_check_latency_ms: Option<i64>,
    check_interval: Option<i64>,
}

impl ServiceRow {
    fn to_service(&self) -> Result<Service> {
        let status = self
            .status
            .as_deref()
            .unwrap_or("unknown")
            .parse()
            .unwrap_or(ServiceStatus::Unknown);

        Ok(Service {
            id: self.id.clone(),
            device_id: self.device_id.clone(),
            port: self.port,
            protocol: self.protocol.clone(),
            name: self.name.clone(),
            status,
            source: self.source.clone(),
            confidence: self.confidence.clone(),
            observed_at: self.observed_at.as_ref().and_then(|s| s.parse().ok()),
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
            last_check_at: self.last_check_at.as_ref().and_then(|s| s.parse().ok()),
            last_check_success: self.last_check_success,
            last_check_latency_ms: self.last_check_latency_ms,
            check_interval: self.check_interval.unwrap_or(60),
        })
    }
}

/// Service store for managing service status
pub struct ServiceStore {
    pool: SqlitePool,
}

impl ServiceStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create or update a service
    pub async fn upsert(&self, service: &Service) -> Result<Service> {
        use sqlx::query;

        let now = Utc::now().to_rfc3339();
        let observed_at = service.observed_at.map(|dt| dt.to_rfc3339());
        let last_check_at = service.last_check_at.map(|dt| dt.to_rfc3339());

        query(
            r#"
            INSERT INTO services (
                id, device_id, port, protocol, name, status, source, confidence,
                observed_at, created_at, updated_at,
                last_check_at, last_check_success, last_check_latency_ms, check_interval
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                device_id = excluded.device_id,
                port = excluded.port,
                protocol = excluded.protocol,
                name = excluded.name,
                status = excluded.status,
                source = excluded.source,
                confidence = excluded.confidence,
                observed_at = excluded.observed_at,
                updated_at = excluded.updated_at,
                last_check_at = excluded.last_check_at,
                last_check_success = excluded.last_check_success,
                last_check_latency_ms = excluded.last_check_latency_ms,
                check_interval = excluded.check_interval
            "#,
        )
        .bind(&service.id)
        .bind(&service.device_id)
        .bind(service.port)
        .bind(&service.protocol)
        .bind(&service.name)
        .bind(service.status.to_string())
        .bind(&service.source)
        .bind(&service.confidence)
        .bind(&observed_at)
        .bind(service.created_at.to_rfc3339())
        .bind(&now)
        .bind(&last_check_at)
        .bind(service.last_check_success)
        .bind(service.last_check_latency_ms)
        .bind(service.check_interval)
        .execute(&self.pool)
        .await?;

        Ok(service.clone())
    }

    /// Get a service by ID
    pub async fn get(&self, id: &str) -> Result<Option<Service>> {
        use sqlx::query_as;

        let row = query_as::<_, ServiceRow>("SELECT * FROM services WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        row.map(|r| r.to_service()).transpose()
    }

    /// Get all services for a device
    pub async fn get_by_device(&self, device_id: &str) -> Result<Vec<Service>> {
        use sqlx::query_as;

        let rows = query_as::<_, ServiceRow>("SELECT * FROM services WHERE device_id = ?")
            .bind(device_id)
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(|r| r.to_service()).collect()
    }

    /// Get all services
    pub async fn list(&self) -> Result<Vec<Service>> {
        use sqlx::query_as;

        let rows = query_as::<_, ServiceRow>("SELECT * FROM services ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(|r| r.to_service()).collect()
    }

    /// Update service status after a check
    pub async fn update_check_result(
        &self,
        service_id: &str,
        success: bool,
        latency_ms: Option<i64>,
    ) -> Result<()> {
        use sqlx::query;

        let now = Utc::now().to_rfc3339();
        let status = if success {
            ServiceStatus::Up.to_string()
        } else {
            ServiceStatus::Down.to_string()
        };

        query(
            r#"
            UPDATE services SET
                status = ?,
                last_check_at = ?,
                last_check_success = ?,
                last_check_latency_ms = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&status)
        .bind(&now)
        .bind(success)
        .bind(latency_ms)
        .bind(&now)
        .bind(service_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get services that need checking (based on check_interval)
    pub async fn get_due_for_check(&self) -> Result<Vec<Service>> {
        use sqlx::query_as;

        let rows = query_as::<_, ServiceRow>(
            r#"
            SELECT * FROM services
            WHERE last_check_at IS NULL
               OR datetime(last_check_at, '+' || check_interval || ' seconds') <= datetime('now')
            ORDER BY last_check_at ASC NULLS FIRST
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.to_service()).collect()
    }

    /// Get service count by status
    pub async fn count_by_status(&self) -> Result<HashMap<String, i64>> {
        use sqlx::query;

        let rows = query("SELECT status, COUNT(*) as count FROM services GROUP BY status")
            .fetch_all(&self.pool)
            .await?;

        let mut counts = HashMap::new();
        for row in rows {
            let status: String = row.get("status");
            let count: i64 = row.get("count");
            counts.insert(status, count);
        }
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_service_upsert_and_get() {
        let tmp = NamedTempFile::new().unwrap();
        let pool = SqlitePool::connect(&format!("sqlite://{}", tmp.path().display()))
            .await
            .unwrap();

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        // Create a device first (required for FK constraint)
        sqlx::query(
            r#"
            INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind("dev-1")
        .bind("test-device")
        .bind("192.168.1.1")
        .bind("aa:bb:cc:dd:ee:ff")
        .bind("router")
        .bind("test")
        .bind("high")
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let store = ServiceStore::new(pool);

        let service = Service {
            id: "svc-1".to_string(),
            device_id: "dev-1".to_string(),
            port: 80,
            protocol: "http".to_string(),
            name: Some("web".to_string()),
            status: ServiceStatus::Unknown,
            source: Some("config".to_string()),
            confidence: Some("high".to_string()),
            observed_at: Some(Utc::now()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_check_at: None,
            last_check_success: None,
            last_check_latency_ms: None,
            check_interval: 60,
        };

        store.upsert(&service).await.unwrap();
        let retrieved = store.get("svc-1").await.unwrap().unwrap();

        assert_eq!(retrieved.id, "svc-1");
        assert_eq!(retrieved.port, 80);
        assert_eq!(retrieved.protocol, "http");
    }
}
