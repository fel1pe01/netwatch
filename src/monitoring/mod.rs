pub mod icmp;
pub mod metrics;
pub mod tcp;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct HealthCheckResult {
    pub target: String,
    pub protocol: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub success: bool,
    pub latency_ms: Option<u128>,
    pub error: Option<String>,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub timeout_ms: u64,
    pub evidence: String,
}

pub use icmp::ping;
pub use tcp::check_tcp;
