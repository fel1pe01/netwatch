use crate::monitoring::HealthCheckResult;
use crate::ssrf::SsrfPolicy;
use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tokio::net::lookup_host;
use tracing::{debug, warn};

/// Validated target for monitoring checks
#[derive(Debug, Clone)]
pub struct ValidatedTarget {
    pub hostname: String,
    pub port: u16,
    pub resolved_ips: Vec<SocketAddr>,
}

impl ValidatedTarget {
    /// Create a validated target from hostname and port using the SSRF policy
    pub async fn new(policy: &SsrfPolicy, hostname: &str, port: u16) -> Result<Self> {
        // Validate hostname
        if !policy.is_hostname_allowed(hostname) {
            anyhow::bail!("Hostname '{}' not allowed by SSRF policy", hostname);
        }

        // Validate port
        if !policy.is_port_allowed(port) {
            anyhow::bail!("Port {} not allowed by SSRF policy", port);
        }

        // Resolve with timeout
        let dns_timeout = Duration::from_millis(policy.config.dns_timeout_ms);
        let lookup_future = lookup_host(format!("{}:{}", hostname, port));
        let addrs = tokio::time::timeout(dns_timeout, lookup_future)
            .await
            .context("DNS resolution timeout")?
            .with_context(|| format!("Failed to resolve {}", hostname))?;

        let mut resolved_ips = Vec::new();
        for addr in addrs {
            if policy.is_ip_allowed(addr.ip()) && policy.is_port_allowed(addr.port()) {
                resolved_ips.push(addr);
            } else {
                warn!("Resolved address {} rejected by SSRF policy", addr);
            }
        }

        if resolved_ips.is_empty() {
            anyhow::bail!(
                "No allowed addresses resolved for {} (all resolved IPs blocked by SSRF policy)",
                hostname
            );
        }

        debug!(
            "Resolved {} to {} allowed addresses",
            hostname,
            resolved_ips.len()
        );

        Ok(Self {
            hostname: hostname.to_string(),
            port,
            resolved_ips,
        })
    }

    /// Create a validated target from an IP address directly
    pub fn from_ip(policy: &SsrfPolicy, ip: IpAddr, port: u16) -> Result<Self> {
        let addr = SocketAddr::new(ip, port);
        policy
            .validate_socket_addr(&addr)
            .context("IP address validation failed")?;

        Ok(Self {
            hostname: ip.to_string(),
            port,
            resolved_ips: vec![addr],
        })
    }

    /// Get the first resolved IP address
    pub fn primary_addr(&self) -> Option<SocketAddr> {
        self.resolved_ips.first().copied()
    }

    /// Get all resolved IP addresses
    pub fn all_addrs(&self) -> &[SocketAddr] {
        &self.resolved_ips
    }
}

/// Validate an HTTP/HTTPS URL for synthetic checks
pub fn validate_http_url(policy: &SsrfPolicy, url: &str) -> Result<ValidatedTarget> {
    use url::Url;

    let parsed = Url::parse(url).context("Invalid URL")?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        anyhow::bail!("Only HTTP and HTTPS URLs are allowed");
    }

    let hostname = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No hostname in URL"))?;

    let port = parsed
        .port()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });

    // For now, we validate synchronously; async resolution happens at check time
    if !policy.is_hostname_allowed(hostname) {
        anyhow::bail!("Hostname '{}' not allowed by SSRF policy", hostname);
    }

    if !policy.is_port_allowed(port) {
        anyhow::bail!("Port {} not allowed by SSRF policy", port);
    }

    Ok(ValidatedTarget {
        hostname: hostname.to_string(),
        port,
        resolved_ips: vec![], // Will be resolved at check time
    })
}

/// Validate a redirect URL during HTTP follow
pub fn validate_redirect_url(policy: &SsrfPolicy, url: &str) -> Result<()> {
    use url::Url;

    let parsed = Url::parse(url).context("Invalid redirect URL")?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        anyhow::bail!("Redirect to non-HTTP(S) scheme: {}", scheme);
    }

    let hostname = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No hostname in redirect URL"))?;

    let port = parsed
        .port()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });

    if !policy.is_hostname_allowed(hostname) {
        anyhow::bail!(
            "Redirect hostname '{}' not allowed by SSRF policy",
            hostname
        );
    }

    if !policy.is_port_allowed(port) {
        anyhow::bail!("Redirect port {} not allowed by SSRF policy", port);
    }

    Ok(())
}

/// Create a HealthCheckResult for a validation error
pub fn validation_error_result(
    target: &str,
    protocol: &str,
    timeout_ms: u64,
    error: impl std::fmt::Display,
) -> HealthCheckResult {
    use chrono::Utc;
    HealthCheckResult {
        target: target.to_string(),
        protocol: protocol.to_string(),
        timestamp: Utc::now(),
        success: false,
        latency_ms: None,
        error: Some(format!("SSRF validation failed: {}", error)),
        observed_at: Utc::now(),
        timeout_ms,
        evidence: "ssrf_validation".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ssrf::SsrfConfig;

    #[tokio::test]
    async fn test_validated_target_allowed() {
        let config = SsrfConfig {
            allow_private_ips: true,
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        let target = ValidatedTarget::new(&policy, "127.0.0.1", 8080).await;
        // With allow_private_ips but not allow_loopback, this should fail
        assert!(target.is_err());
    }

    #[tokio::test]
    async fn test_validated_target_from_ip() {
        let config = SsrfConfig {
            allow_private_ips: true,
            allow_loopback: true,
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        let target = ValidatedTarget::from_ip(&policy, "127.0.0.1".parse().unwrap(), 8080).unwrap();
        assert_eq!(target.hostname, "127.0.0.1");
        assert_eq!(target.port, 8080);
        assert_eq!(target.resolved_ips.len(), 1);
    }

    #[test]
    fn test_validate_http_url() {
        let config = SsrfConfig {
            allowed_hostnames: vec!["example.com".to_string()],
            allowed_ports: vec![80, 443],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        let target = validate_http_url(&policy, "http://example.com").unwrap();
        assert_eq!(target.hostname, "example.com");
        assert_eq!(target.port, 80);

        let target = validate_http_url(&policy, "https://example.com:8443").unwrap();
        assert_eq!(target.hostname, "example.com");
        assert_eq!(target.port, 8443);
    }

    #[test]
    fn test_validate_http_url_rejected() {
        let config = SsrfConfig {
            allowed_hostnames: vec!["example.com".to_string()],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        let result = validate_http_url(&policy, "http://evil.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_redirect_url() {
        let config = SsrfConfig {
            allowed_hostnames: vec!["example.com".to_string()],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(validate_redirect_url(&policy, "http://example.com/path").is_ok());
        assert!(validate_redirect_url(&policy, "https://example.com/path").is_ok());
        assert!(validate_redirect_url(&policy, "http://evil.com/path").is_err());
    }
}
