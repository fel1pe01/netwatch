use crate::monitoring::HealthCheckResult;
use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use std::process::Stdio;
use tokio::process::Command;

/// Perform a single ICMP echo request (ping) to the specified host.
/// Returns a structured `HealthCheckResult`.
pub async fn ping(target: &str, timeout_ms: u64) -> Result<HealthCheckResult> {
    let timeout_seconds = timeout_ms.div_ceil(1000); // ceil to seconds
    let output = Command::new("ping")
        .args(["-c", "1", "-w", &timeout_seconds.to_string(), target])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    let now = Utc::now();
    let (success, error, latency_ms) = match output {
        Ok(out) => {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let re = Regex::new(r"time=([\d\.]+)").unwrap();
                let latency = re
                    .captures(&stdout)
                    .and_then(|caps| caps.get(1))
                    .and_then(|m| m.as_str().parse::<f64>().ok());
                let latency_ms_opt = latency.map(|t| (t * 1000.0) as u128);
                (true, None, latency_ms_opt)
            } else {
                // Prefer stderr, fall back to stdout for a useful error message
                let error_msg = if !out.stderr.is_empty() {
                    String::from_utf8_lossy(&out.stderr).to_string()
                } else {
                    String::from_utf8_lossy(&out.stdout).to_string()
                };
                (false, Some(error_msg.trim().to_string()), None)
            }
        }
        Err(e) => (false, Some(e.to_string()), None),
    };

    Ok(HealthCheckResult {
        target: target.to_string(),
        protocol: "icmp".to_string(),
        timestamp: now,
        success,
        latency_ms,
        error,
        observed_at: now,
        timeout_ms,
        evidence: "icmp".to_string(),
    })
}
