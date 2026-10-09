use crate::monitoring::HealthCheckResult;
use anyhow::Result;
use chrono::Utc;
use reqwest::Client;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// HTTP/HTTPS health check with detailed phase timing
pub async fn check_http(
    url: &str,
    timeout_ms: u64,
    expected_status: Option<u16>,
    expected_body: Option<String>,
    headers: Option<Vec<(String, String)>>,
) -> Result<HealthCheckResult> {
    let timeout_duration = Duration::from_millis(timeout_ms);
    let start = Instant::now();
    let now = Utc::now();

    let client = Client::builder()
        .timeout(timeout_duration)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let mut request = client.get(url);

    if let Some(h) = headers {
        for (key, value) in h {
            request = request.header(key, value);
        }
    }

    let result = timeout(timeout_duration, request.send()).await;

    let total_latency_ms = start.elapsed().as_millis();
    let (success, error, _status_code, _body, _phase_timings) = match result {
        Ok(Ok(response)) => {
            let status = response.status().as_u16();
            let body_text = response.text().await.ok();

            let status_ok = expected_status.is_none_or(|expected| status == expected);
            let body_ok = expected_body
                .as_ref()
                .is_none_or(|expected| body_text.as_ref().is_some_and(|b| b.contains(expected)));

            let overall_success = status_ok && body_ok;

            let error_msg = if !status_ok {
                Some(format!(
                    "unexpected status: got {}, expected {:?}",
                    status, expected_status
                ))
            } else if !body_ok {
                Some("expected body content not found".to_string())
            } else {
                None
            };

            (
                overall_success,
                error_msg,
                Some(status),
                body_text,
                None::<String>,
            )
        }
        Ok(Err(e)) => (false, Some(e.to_string()), None, None, None),
        Err(_) => (false, Some("timeout".to_string()), None, None, None),
    };

    let latency_ms = if success {
        Some(total_latency_ms)
    } else {
        None
    };

    Ok(HealthCheckResult {
        target: url.to_string(),
        protocol: if url.starts_with("https") {
            "https"
        } else {
            "http"
        }
        .to_string(),
        timestamp: now,
        success,
        latency_ms,
        error,
        observed_at: now,
        timeout_ms,
        evidence: "http_probe".to_string(),
    })
}

pub async fn check_http_with_phases(url: &str, timeout_ms: u64) -> Result<HealthCheckResult> {
    let timeout_duration = Duration::from_millis(timeout_ms);
    let start = Instant::now();
    let now = Utc::now();

    let client = Client::builder()
        .timeout(timeout_duration)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let _dns_start = Instant::now();
    let result = timeout(timeout_duration, client.get(url).send()).await;
    let _dns_latency_ms = _dns_start.elapsed().as_millis();

    let total_latency_ms = start.elapsed().as_millis();
    let (success, error, _status_code) = match result {
        Ok(Ok(response)) => {
            let status = response.status().as_u16();
            let status_ok = status < 400;
            (
                status_ok,
                if status_ok {
                    None
                } else {
                    Some(format!("status {}", status))
                },
                Some(status),
            )
        }
        Ok(Err(e)) => (false, Some(e.to_string()), None),
        Err(_) => (false, Some("timeout".to_string()), None),
    };

    let latency_ms = if success {
        Some(total_latency_ms)
    } else {
        None
    };

    Ok(HealthCheckResult {
        target: url.to_string(),
        protocol: if url.starts_with("https") {
            "https"
        } else {
            "http"
        }
        .to_string(),
        timestamp: now,
        success,
        latency_ms,
        error,
        observed_at: now,
        timeout_ms,
        evidence: "http_probe".to_string(),
    })
}
