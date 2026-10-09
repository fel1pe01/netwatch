use crate::monitoring::HealthCheckResult;
use anyhow::Result;
use chrono::Utc;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;

/// TCP connectivity check.
pub async fn check_tcp(host: &str, port: u16, timeout_ms: u64) -> Result<HealthCheckResult> {
    let addr = format!("{host}:{port}");
    let sock_addr: SocketAddr = addr.parse()?;
    let timeout_duration = Duration::from_millis(timeout_ms);
    let start = std::time::Instant::now();
    let result = timeout(timeout_duration, TcpStream::connect(sock_addr)).await;
    let elapsed_ms = start.elapsed().as_millis();
    let (success, error) = match result {
        Ok(Ok(_stream)) => (true, None),
        Ok(Err(e)) => (false, Some(e.to_string())),
        Err(_) => (false, Some("timeout".to_string())),
    };
    let latency_ms = if success { Some(elapsed_ms) } else { None };
    let now = Utc::now();
    Ok(HealthCheckResult {
        target: addr,
        protocol: "tcp".to_string(),
        timestamp: now,
        success,
        latency_ms,
        error,
        observed_at: now,
        timeout_ms,
        evidence: "tcp_probe".to_string(),
    })
}
