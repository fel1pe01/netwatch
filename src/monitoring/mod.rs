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
pub use http::{check_http, check_http_with_phases, HttpPhaseTimings};
pub use icmp::ping;
pub use metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
    NETWATCH_SCHEDULER_ERRORS_TOTAL, NETWATCH_SCHEDULER_JOBS_TOTAL,
    SYNTHETIC_CHECK_DNS_DURATION_SECONDS, SYNTHETIC_CHECK_TCP_DURATION_SECONDS,
    SYNTHETIC_CHECK_TLS_DURATION_SECONDS, SYNTHETIC_CHECK_TOTAL,
    SYNTHETIC_CHECK_TOTAL_DURATION_SECONDS, SYNTHETIC_CHECK_TTFB_DURATION_SECONDS,
};
pub use tcp::check_tcp;
pub use tls::{check_tls, check_tls_full};
