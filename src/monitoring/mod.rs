pub mod dns;
pub mod http;
pub mod icmp;
pub mod metrics;
pub mod tcp;
pub mod tls;

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

pub use dns::check_dns;
pub use http::{check_http, check_http_with_phases};
pub use icmp::ping;
pub use tcp::check_tcp;
pub use tls::{check_tls, check_tls_full};
