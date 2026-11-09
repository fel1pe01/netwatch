use crate::monitoring::validation::{validation_error_result, ValidatedTarget};
use crate::monitoring::HealthCheckResult;
use crate::ssrf::SsrfPolicy;
use anyhow::{Context, Result};
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

/// ICMP echo request (ping) with SSRF validation
pub async fn ping_with_ssrf(
    policy: &SsrfPolicy,
    target: &str,
    timeout_ms: u64,
) -> Result<HealthCheckResult> {
    // Validate target against SSRF policy (ICMP uses port 0 conceptually)
    let validated = ValidatedTarget::new(policy, target, 0).await?;

    // Use the first resolved IP for the actual ping
    let target_ip = validated
        .primary_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| target.to_string());

    let timeout_seconds = timeout_ms.div_ceil(1000); // ceil to seconds
    let output = Command::new("ping")
        .args(["-c", "1", "-w", &timeout_seconds.to_string(), &target_ip])
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
        target: target.to_string(), // Keep original target for reporting
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
