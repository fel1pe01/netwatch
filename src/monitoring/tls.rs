use crate::monitoring::HealthCheckResult;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rustls::pki_types::ServerName;
use rustls::RootCertStore;
use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

/// TLS certificate health check
pub async fn check_tls(
    hostname: &str,
    port: u16,
    timeout_ms: u64,
    check_expiry_days: Option<u64>,
    verify_hostname: bool,
) -> Result<HealthCheckResult> {
    let timeout_duration = StdDuration::from_millis(timeout_ms);
    let start = Instant::now();
    let now = Utc::now();

    let addr = format!("{}:{}", hostname, port);
    let server_name = ServerName::try_from(hostname)?.to_owned();

    // Load native CA certificates
    let mut root_store = RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs().expect("failed to load native certs");
    for cert in certs {
        root_store.add(cert)?;
    }

    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let connector = TlsConnector::from(Arc::new(config));

    let tcp_stream = timeout(timeout_duration, TcpStream::connect(&addr)).await??;
    let tls_stream =
        timeout(timeout_duration, connector.connect(server_name, tcp_stream)).await??;

    let (_, connection) = tls_stream.get_ref();
    let peer_certs_opt = connection.peer_certificates();

    let latency_ms = start.elapsed().as_millis();

    let (success, error, _cert_info) = if let Some(peer_certs) = peer_certs_opt {
        if let Some(cert) = peer_certs.first() {
            let x509 = x509_parser::parse_x509_certificate(cert.as_ref())?.1;

            let not_after = DateTime::from_timestamp(x509.validity().not_after.timestamp(), 0)
                .unwrap_or(Utc::now());
            let days_until_expiry = (not_after - Utc::now()).num_days();

            let expiry_ok = check_expiry_days.is_none_or(|days| days_until_expiry >= days as i64);

            let hostname_ok = if verify_hostname {
                match x509.subject_alternative_name() {
                    Ok(Some(san)) => san.value.general_names.iter().any(|name| {
                        matches!(name, x509_parser::extensions::GeneralName::DNSName(dns) if **dns == *hostname)
                    }),
                    Ok(None) => false,
                    Err(_) => false,
                }
            } else {
                true
            };

            let overall_success = expiry_ok && hostname_ok;

            let error_msg = if !expiry_ok {
                Some(format!("certificate expires in {} days", days_until_expiry))
            } else if !hostname_ok {
                Some("hostname verification failed".to_string())
            } else {
                None
            };

            let cert_info = Some(serde_json::json!({
                "subject": x509.subject().to_string(),
                "issuer": x509.issuer().to_string(),
                "not_before": x509.validity().not_before.to_string(),
                "not_after": x509.validity().not_after.to_string(),
                "days_until_expiry": days_until_expiry,
                "serial_number": x509.serial.to_string(),
                "signature_algorithm": x509.signature_algorithm.oid().to_string(),
            }));

            (overall_success, error_msg, cert_info)
        } else {
            (
                false,
                Some("no peer certificate in chain".to_string()),
                None,
            )
        }
    } else {
        (false, Some("no peer certificate".to_string()), None)
    };

    let latency = if success { Some(latency_ms) } else { None };

    Ok(HealthCheckResult {
        target: format!("{}:{}", hostname, port),
        protocol: "tls".to_string(),
        timestamp: now,
        success,
        latency_ms: latency,
        error,
        observed_at: now,
        timeout_ms,
        evidence: "tls_probe".to_string(),
    })
}

/// TLS check with certificate chain validation
pub async fn check_tls_full(
    hostname: &str,
    port: u16,
    timeout_ms: u64,
) -> Result<HealthCheckResult> {
    check_tls(hostname, port, timeout_ms, Some(30), true).await
}
