use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

/// TLS configuration for the HTTP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS (default: false)
    pub enabled: bool,
    /// Path to certificate file (PEM format)
    pub cert_file: Option<String>,
    /// Path to private key file (PEM format)
    pub key_file: Option<String>,
    /// Minimum TLS version (default: "1.2")
    #[serde(default = "default_min_version")]
    pub min_version: String,
    /// Whether to require client certificate authentication (mTLS)
    #[serde(default)]
    pub require_client_auth: bool,
    /// Path to CA certificate file for client verification (PEM format)
    pub ca_file: Option<String>,
}

fn default_min_version() -> String {
    "1.2".to_string()
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_file: None,
            key_file: None,
            min_version: default_min_version(),
            require_client_auth: false,
            ca_file: None,
        }
    }
}

impl TlsConfig {
    /// Load and validate TLS configuration, returning a ServerConfig if enabled
    pub fn load_server_config(&self) -> Result<Option<Arc<ServerConfig>>> {
        if !self.enabled {
            return Ok(None);
        }

        let cert_file = self
            .cert_file
            .as_ref()
            .context("TLS enabled but cert_file not configured")?;
        let key_file = self
            .key_file
            .as_ref()
            .context("TLS enabled but key_file not configured")?;

        // Load certificate chain
        let cert_chain = load_certs(cert_file)
            .with_context(|| format!("Failed to load certificates from {}", cert_file))?;

        // Load private key
        let private_key = load_private_key(key_file)
            .with_context(|| format!("Failed to load private key from {}", key_file))?;

        // Build server config
        let mut config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .context("Failed to create TLS server config with certificate")?;

        // Configure minimum TLS version
        config.max_early_data_size = 0;

        // Set ALPN protocols for HTTP/2 and HTTP/1.1
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        // Configure client auth if required
        if self.require_client_auth {
            let ca_file = self
                .ca_file
                .as_ref()
                .context("mTLS enabled but ca_file not configured")?;
            let ca_certs = load_certs(ca_file)
                .with_context(|| format!("Failed to load CA certificates from {}", ca_file))?;

            let mut root_store = rustls::RootCertStore::empty();
            for cert in ca_certs {
                root_store
                    .add(cert)
                    .context("Failed to add CA certificate to root store")?;
            }

            let client_auth = rustls::server::WebPkiClientVerifier::builder(root_store.into())
                .build()
                .context("Failed to build client verifier")?;

            config = ServerConfig::builder()
                .with_client_cert_verifier(client_auth)
                .with_single_cert(
                    config.cert_resolver.unwrap().clone_cert_resolver(),
                    config.key_log.clone(),
                )
                .context("Failed to create TLS server config with client auth")?;
        }

        Ok(Some(Arc::new(config)))
    }

    /// Validate configuration without loading certificates
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if self.cert_file.is_none() {
            anyhow::bail!("TLS enabled but cert_file not configured");
        }
        if self.key_file.is_none() {
            anyhow::bail!("TLS enabled but key_file not configured");
        }
        if self.require_client_auth && self.ca_file.is_none() {
            anyhow::bail!("mTLS enabled but ca_file not configured");
        }

        Ok(())
    }
}

/// Load certificates from a PEM file
fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let content =
        fs::read(path).with_context(|| format!("Failed to read certificate file: {}", path))?;

    let certs = rustls_pemfile::certs(&mut content.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse certificates")?;

    if certs.is_empty() {
        anyhow::bail!("No certificates found in {}", path);
    }

    Ok(certs)
}

/// Load private key from a PEM file (supports PKCS#8 and PKCS#1)
fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let content =
        fs::read(path).with_context(|| format!("Failed to read private key file: {}", path))?;

    // Try PKCS#8 first
    let mut reader = content.as_slice();
    if let Ok(Some(key)) = rustls_pemfile::private_key(&mut reader) {
        return Ok(key);
    }

    // Try PKCS#1 (RSA)
    let mut reader = content.as_slice();
    if let Ok(Some(key)) = rustls_pemfile::rsa_private_key(&mut reader) {
        return Ok(key.into());
    }

    // Try PKCS#1 (EC)
    let mut reader = content.as_slice();
    if let Ok(Some(key)) = rustls_pemfile::ec_private_key(&mut reader) {
        return Ok(key.into());
    }

    anyhow::bail!(
        "Failed to parse private key from {} (tried PKCS#8, PKCS#1 RSA, PKCS#1 EC)",
        path
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tls_config_default() {
        let config = TlsConfig::default();
        assert!(!config.enabled);
        assert!(config.cert_file.is_none());
        assert!(config.key_file.is_none());
        assert!(!config.require_client_auth);
    }

    #[test]
    fn test_tls_config_validation_disabled() {
        let config = TlsConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tls_config_validation_missing_cert() {
        let config = TlsConfig {
            enabled: true,
            cert_file: None,
            key_file: Some("key.pem".to_string()),
            min_version: "1.2".to_string(),
            require_client_auth: false,
            ca_file: None,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_tls_config_validation_missing_key() {
        let config = TlsConfig {
            enabled: true,
            cert_file: Some("cert.pem".to_string()),
            key_file: None,
            min_version: "1.2".to_string(),
            require_client_auth: false,
            ca_file: None,
        };
        assert!(config.validate().is_err());
    }
}
