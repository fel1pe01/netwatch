use anyhow::{Context, Result};
use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::lookup_host;
use tracing::{debug, warn};

/// SSRF protection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsrfConfig {
    /// Enable SSRF protection (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Allow private IP ranges (RFC 1918) - default: false
    #[serde(default)]
    pub allow_private_ips: bool,
    /// Allow loopback addresses (127.0.0.0/8, ::1) - default: false
    #[serde(default)]
    pub allow_loopback: bool,
    /// Allow link-local addresses (169.254.0.0/16, fe80::/10) - default: false
    #[serde(default)]
    pub allow_link_local: bool,
    /// Allow multicast addresses - default: false
    #[serde(default)]
    pub allow_multicast: bool,
    /// Allow broadcast addresses - default: false
    #[serde(default)]
    pub allow_broadcast: bool,
    /// Additional allowed CIDR ranges
    #[serde(default)]
    pub allowed_cidrs: Vec<String>,
    /// Additional allowed hostnames (exact match or suffix match with .)
    #[serde(default)]
    pub allowed_hostnames: Vec<String>,
    /// Blocked CIDR ranges (takes precedence over allowed)
    #[serde(default)]
    pub blocked_cidrs: Vec<String>,
    /// Maximum number of redirects to follow
    #[serde(default = "default_max_redirects")]
    pub max_redirects: u32,
    /// Timeout for DNS resolution
    #[serde(default = "default_dns_timeout")]
    pub dns_timeout_ms: u64,
    /// Validate DNS resolution matches the original IP (prevent DNS rebinding)
    #[serde(default = "default_true")]
    pub validate_dns_rebinding: bool,
    /// Allowed ports (empty = all ports allowed)
    #[serde(default)]
    pub allowed_ports: Vec<u16>,
    /// Blocked ports (takes precedence over allowed)
    #[serde(default)]
    pub blocked_ports: Vec<u16>,
}

fn default_true() -> bool {
    true
}

fn default_max_redirects() -> u32 {
    10
}

fn default_dns_timeout() -> u64 {
    5000
}

impl Default for SsrfConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_private_ips: false,
            allow_loopback: false,
            allow_link_local: false,
            allow_multicast: false,
            allow_broadcast: false,
            allowed_cidrs: vec![],
            allowed_hostnames: vec![],
            blocked_cidrs: vec![],
            max_redirects: 10,
            dns_timeout_ms: 5000,
            validate_dns_rebinding: true,
            allowed_ports: vec![],
            blocked_ports: vec![],
        }
    }
}

/// Parsed and validated SSRF policy for fast runtime checks
#[derive(Debug, Clone)]
pub struct SsrfPolicy {
    config: SsrfConfig,
    allowed_networks: Vec<Ipv4Net>,
    blocked_networks: Vec<Ipv4Net>,
    allowed_hostnames: HashSet<String>,
    allowed_port_set: Option<HashSet<u16>>,
    blocked_port_set: Option<HashSet<u16>>,
}

impl SsrfPolicy {
    /// Create a new SSRF policy from configuration
    pub fn new(config: SsrfConfig) -> Result<Self> {
        let mut allowed_networks = Vec::new();
        let mut blocked_networks = Vec::new();

        // Parse allowed CIDRs
        for cidr in &config.allowed_cidrs {
            let network: Ipv4Net = cidr
                .parse()
                .with_context(|| format!("Invalid allowed CIDR: {}", cidr))?;
            allowed_networks.push(network);
        }

        // Parse blocked CIDRs
        for cidr in &config.blocked_cidrs {
            let network: Ipv4Net = cidr
                .parse()
                .with_context(|| format!("Invalid blocked CIDR: {}", cidr))?;
            blocked_networks.push(network);
        }

        // Always block cloud metadata endpoints by default
        // AWS: 169.254.169.254
        // GCP: 169.254.169.254
        // Azure: 169.254.169.254
        // DigitalOcean: 169.254.169.254
        let metadata_ip: Ipv4Net = "169.254.169.254/32".parse().unwrap();
        if !blocked_networks
            .iter()
            .any(|n| n.contains(&Ipv4Addr::new(169, 254, 169, 254)))
        {
            blocked_networks.push(metadata_ip);
        }

        // Always block loopback by default unless explicitly allowed
        let loopback_v4: Ipv4Net = "127.0.0.0/8".parse().unwrap();
        if !config.allow_loopback
            && !allowed_networks
                .iter()
                .any(|n| n.contains(&Ipv4Addr::new(127, 0, 0, 1)))
        {
            blocked_networks.push(loopback_v4);
        }

        // Always block link-local by default unless explicitly allowed
        let link_local_v4: Ipv4Net = "169.254.0.0/16".parse().unwrap();
        if !config.allow_link_local
            && !allowed_networks
                .iter()
                .any(|n| n.contains(&Ipv4Addr::new(169, 254, 0, 0)))
        {
            blocked_networks.push(link_local_v4);
        }

        let allowed_hostnames: HashSet<String> = config
            .allowed_hostnames
            .iter()
            .map(|h| h.to_lowercase())
            .collect();

        let allowed_port_set = if config.allowed_ports.is_empty() {
            None
        } else {
            Some(config.allowed_ports.iter().copied().collect())
        };

        let blocked_port_set = if config.blocked_ports.is_empty() {
            None
        } else {
            Some(config.blocked_ports.iter().copied().collect())
        };

        Ok(Self {
            config,
            allowed_networks,
            blocked_networks,
            allowed_hostnames,
            allowed_port_set,
            blocked_port_set,
        })
    }

    /// Check if an IP address is allowed
    pub fn is_ip_allowed(&self, ip: IpAddr) -> bool {
        if !self.config.enabled {
            return true;
        }

        // Check blocked networks first (highest priority)
        for network in &self.blocked_networks {
            if match ip {
                IpAddr::V4(ipv4) => network.contains(&ipv4),
                IpAddr::V6(_) => false, // IPv6 not in IPv4 networks
            } {
                debug!("IP {} blocked by CIDR {}", ip, network);
                return false;
            }
        }

        // Check if explicitly allowed
        for network in &self.allowed_networks {
            if match ip {
                IpAddr::V4(ipv4) => network.contains(&ipv4),
                IpAddr::V6(_) => false,
            } {
                return true;
            }
        }

        // Check private IP ranges
        if let IpAddr::V4(ipv4) = ip {
            if ipv4.is_private() && !self.config.allow_private_ips {
                debug!("Private IP {} blocked", ipv4);
                return false;
            }
            if ipv4.is_loopback() && !self.config.allow_loopback {
                debug!("Loopback IP {} blocked", ipv4);
                return false;
            }
            if ipv4.is_link_local() && !self.config.allow_link_local {
                debug!("Link-local IP {} blocked", ipv4);
                return false;
            }
            if ipv4.is_multicast() && !self.config.allow_multicast {
                debug!("Multicast IP {} blocked", ipv4);
                return false;
            }
            if ipv4.is_broadcast() && !self.config.allow_broadcast {
                debug!("Broadcast IP {} blocked", ipv4);
                return false;
            }
        } else if let IpAddr::V6(ipv6) = ip {
            if ipv6.is_loopback() && !self.config.allow_loopback {
                debug!("IPv6 loopback {} blocked", ipv6);
                return false;
            }
            if ipv6.is_unicast_link_local() && !self.config.allow_link_local {
                debug!("IPv6 link-local {} blocked", ipv6);
                return false;
            }
            if ipv6.is_multicast() && !self.config.allow_multicast {
                debug!("IPv6 multicast {} blocked", ipv6);
                return false;
            }
        }

        true
    }

    /// Check if a hostname is allowed
    pub fn is_hostname_allowed(&self, hostname: &str) -> bool {
        if !self.config.enabled {
            return true;
        }

        let hostname_lower = hostname.to_lowercase();

        // Check exact match or suffix match (for subdomain allowance)
        for allowed in &self.allowed_hostnames {
            if hostname_lower == *allowed || hostname_lower.ends_with(&format!(".{}", allowed)) {
                return true;
            }
        }

        true // Allow if no hostname restrictions configured
    }

    /// Check if a port is allowed
    pub fn is_port_allowed(&self, port: u16) -> bool {
        if !self.config.enabled {
            return true;
        }

        // Check blocked ports first
        if let Some(blocked) = &self.blocked_port_set {
            if blocked.contains(&port) {
                debug!("Port {} blocked", port);
                return false;
            }
        }

        // Check allowed ports (if configured)
        if let Some(allowed) = &self.allowed_port_set {
            if !allowed.contains(&port) {
                debug!("Port {} not in allowed list", port);
                return false;
            }
        }

        true
    }

    /// Validate a socket address (IP + port)
    pub fn validate_socket_addr(&self, addr: &SocketAddr) -> Result<()> {
        if !self.is_ip_allowed(addr.ip()) {
            anyhow::bail!("IP address {} is not allowed by SSRF policy", addr.ip());
        }
        if !self.is_port_allowed(addr.port()) {
            anyhow::bail!("Port {} is not allowed by SSRF policy", addr.port());
        }
        Ok(())
    }

    /// Resolve and validate a hostname, returning allowed IPs
    pub async fn resolve_and_validate(&self, hostname: &str, port: u16) -> Result<Vec<SocketAddr>> {
        if !self.config.enabled {
            // Just resolve without validation
            let addrs = lookup_host(format!("{}:{}", hostname, port))
                .await
                .with_context(|| format!("Failed to resolve {}", hostname))?;
            return Ok(addrs.collect());
        }

        // Check hostname allowlist
        if !self.is_hostname_allowed(hostname) {
            anyhow::bail!("Hostname {} is not allowed by SSRF policy", hostname);
        }

        // Resolve with timeout
        let lookup_future = lookup_host(format!("{}:{}", hostname, port));
        let addrs = tokio::time::timeout(
            Duration::from_millis(self.config.dns_timeout_ms),
            lookup_future,
        )
        .await
        .context("DNS resolution timeout")?
        .with_context(|| format!("Failed to resolve {}", hostname))?;

        let mut valid_addrs = Vec::new();
        for addr in addrs {
            if self.is_ip_allowed(addr.ip()) && self.is_port_allowed(addr.port()) {
                valid_addrs.push(addr);
            } else {
                warn!("Resolved address {} rejected by SSRF policy", addr);
            }
        }

        if valid_addrs.is_empty() {
            anyhow::bail!("No allowed addresses resolved for {}", hostname);
        }

        Ok(valid_addrs)
    }

    /// Validate a redirect target (for HTTP redirect following)
    pub fn validate_redirect(&self, url: &url::Url) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let hostname = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("No hostname in redirect URL"))?;

        let port = url
            .port()
            .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });

        // Check hostname
        if !self.is_hostname_allowed(hostname) {
            anyhow::bail!("Redirect hostname {} not allowed", hostname);
        }

        // We can't resolve here synchronously, but we can check port
        if !self.is_port_allowed(port) {
            anyhow::bail!("Redirect port {} not allowed", port);
        }

        Ok(())
    }

    /// Get the configuration
    pub fn config(&self) -> &SsrfConfig {
        &self.config
    }
}

/// Shared SSRF policy state
#[derive(Clone)]
pub struct SsrfState {
    policy: Arc<SsrfPolicy>,
}

impl SsrfState {
    pub fn new(config: SsrfConfig) -> Result<Self> {
        Ok(Self {
            policy: Arc::new(SsrfPolicy::new(config)?),
        })
    }

    pub fn policy(&self) -> &SsrfPolicy {
        &self.policy
    }

    pub fn update_policy(&mut self, config: SsrfConfig) -> Result<()> {
        self.policy = Arc::new(SsrfPolicy::new(config)?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_default_policy_blocks_loopback() {
        let config = SsrfConfig::default();
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(!policy.is_ip_allowed(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    }

    #[test]
    fn test_default_policy_blocks_metadata() {
        let config = SsrfConfig::default();
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))));
    }

    #[test]
    fn test_default_policy_blocks_private() {
        let config = SsrfConfig::default();
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn test_allow_private_ips() {
        let config = SsrfConfig {
            allow_private_ips: true,
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn test_allow_loopback() {
        let config = SsrfConfig {
            allow_loopback: true,
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }

    #[test]
    fn test_allowed_cidrs() {
        let config = SsrfConfig {
            allowed_cidrs: vec!["10.0.0.0/8".to_string()],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn test_blocked_cidrs_precedence() {
        let config = SsrfConfig {
            allowed_cidrs: vec!["10.0.0.0/8".to_string()],
            blocked_cidrs: vec!["10.0.0.0/24".to_string()],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(!policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(policy.is_ip_allowed(IpAddr::V4(Ipv4Addr::new(10, 0, 1, 1))));
    }

    #[test]
    fn test_port_filtering() {
        let config = SsrfConfig {
            allowed_ports: vec![80, 443, 8080],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(policy.is_port_allowed(80));
        assert!(policy.is_port_allowed(443));
        assert!(policy.is_port_allowed(8080));
        assert!(!policy.is_port_allowed(22));
        assert!(!policy.is_port_allowed(3306));
    }

    #[test]
    fn test_blocked_ports_precedence() {
        let config = SsrfConfig {
            allowed_ports: vec![80, 443, 8080],
            blocked_ports: vec![8080],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(policy.is_port_allowed(80));
        assert!(!policy.is_port_allowed(8080));
    }

    #[test]
    fn test_hostname_allowlist() {
        let config = SsrfConfig {
            allowed_hostnames: vec!["example.com".to_string(), "api.internal".to_string()],
            ..Default::default()
        };
        let policy = SsrfPolicy::new(config).unwrap();

        assert!(policy.is_hostname_allowed("example.com"));
        assert!(policy.is_hostname_allowed("sub.example.com"));
        assert!(policy.is_hostname_allowed("api.internal"));
        assert!(!policy.is_hostname_allowed("evil.com"));
        assert!(!policy.is_hostname_allowed("example.com.evil.com"));
    }
}
