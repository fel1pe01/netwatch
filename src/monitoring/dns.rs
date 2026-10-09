use crate::monitoring::HealthCheckResult;
use anyhow::Result;
use chrono::Utc;
use hickory_resolver::Resolver;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// DNS resolution health check
pub async fn check_dns(
    hostname: &str,
    record_type: Option<String>,
    expected_ips: Option<Vec<String>>,
    timeout_ms: u64,
) -> Result<HealthCheckResult> {
    let timeout_duration = Duration::from_millis(timeout_ms);
    let start = Instant::now();
    let now = Utc::now();

    // Use system configuration (resolv.conf on Unix)
    let resolver = Resolver::builder_tokio()
        .map_err(|e| anyhow::anyhow!("Failed to create resolver: {}", e))?
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build resolver: {}", e))?;

    let record_type = record_type.unwrap_or_else(|| "A".to_string());
    let query_type = match record_type.to_uppercase().as_str() {
        "A" => hickory_resolver::proto::rr::RecordType::A,
        "AAAA" => hickory_resolver::proto::rr::RecordType::AAAA,
        "CNAME" => hickory_resolver::proto::rr::RecordType::CNAME,
        "MX" => hickory_resolver::proto::rr::RecordType::MX,
        "TXT" => hickory_resolver::proto::rr::RecordType::TXT,
        "NS" => hickory_resolver::proto::rr::RecordType::NS,
        _ => hickory_resolver::proto::rr::RecordType::A,
    };

    let result = timeout(timeout_duration, resolver.lookup(hostname, query_type)).await;

    let latency_ms = start.elapsed().as_millis();
    let (success, error, _resolved_ips) = match result {
        Ok(Ok(response)) => {
            let ips: Vec<String> = response
                .answers()
                .iter()
                .filter_map(|record| match &record.data {
                    hickory_resolver::proto::rr::RData::A(addr) => Some(addr.to_string()),
                    hickory_resolver::proto::rr::RData::AAAA(addr) => Some(addr.to_string()),
                    hickory_resolver::proto::rr::RData::CNAME(cname) => Some(cname.to_string()),
                    _ => None,
                })
                .collect();

            let ips_ok = expected_ips
                .as_ref()
                .is_none_or(|expected| expected.iter().all(|e| ips.contains(e)));

            (
                ips_ok,
                if ips.is_empty() {
                    Some("no records found".to_string())
                } else {
                    None
                },
                Some(ips),
            )
        }
        Ok(Err(e)) => (false, Some(e.to_string()), None),
        Err(_) => (false, Some("timeout".to_string()), None),
    };

    let latency = if success { Some(latency_ms) } else { None };

    Ok(HealthCheckResult {
        target: format!("{} ({})", hostname, record_type),
        protocol: "dns".to_string(),
        timestamp: now,
        success,
        latency_ms: latency,
        error,
        observed_at: now,
        timeout_ms,
        evidence: "dns_probe".to_string(),
    })
}
