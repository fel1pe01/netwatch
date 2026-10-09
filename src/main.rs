use anyhow::Result;
use axum::serve;
use axum::{routing::get, Router};
use chrono::Utc;
use clap::{Parser, Subcommand};
use netwatch::api::router as api_router;
use netwatch::discovery::interfaces::discover_interfaces;
use netwatch::discovery::routing::discover_routes;
use netwatch::logging::init_logging;
use netwatch::metrics::AppMetrics;
use netwatch::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use netwatch::monitoring::{check_tcp, ping, HealthCheckResult};
use serde_json::to_string;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Netwatch CLI
#[derive(Parser)]
#[command(name = "netwatch", version, about = "Netwatch network monitoring tool")]
struct Cli {
    #[clap(subcommand)]
    command: Option<NetwatchCommand>,
}
#[derive(Subcommand)]
enum NetwatchCommand {
    Discover {
        #[clap(subcommand)]
        kind: DiscoverKind,
    },
    #[command(name = "monitor")]
    Monitor {
        #[clap(subcommand)]
        kind: MonitorKind,
    },
}
#[derive(Subcommand)]
enum DiscoverKind {
    /// Perform local passive discovery
    Local,
}
#[derive(Subcommand)]
enum MonitorKind {
    Ping { target: String },
    Tcp { host: String, port: u16 },
}
#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI
    let cli = Cli::parse();

    match cli.command {
        Some(NetwatchCommand::Monitor { kind }) => {
            match kind {
                MonitorKind::Ping { target } => {
                    let res = ping(&target, 2000)
                        .await
                        .unwrap_or_else(|e| HealthCheckResult {
                            target: target.clone(),
                            protocol: "icmp".to_string(),
                            timestamp: Utc::now(),
                            success: false,
                            latency_ms: None,
                            error: Some(e.to_string()),
                            observed_at: Utc::now(),
                            timeout_ms: 2000,
                            evidence: "icmp".to_string(),
                        });
                    // Record metrics
                    let proto = res.protocol.as_str();
                    HEALTH_CHECK_TOTAL.with_label_values(&[proto]).inc();
                    if !res.success {
                        HEALTH_CHECK_FAILURES_TOTAL
                            .with_label_values(&[proto])
                            .inc();
                    }
                    if let Some(latency) = res.latency_ms {
                        let latency_seconds = latency as f64 / 1000.0;
                        HEALTH_CHECK_LATENCY_SECONDS
                            .with_label_values(&[proto])
                            .observe(latency_seconds);
                    }
                    println!("{}", to_string(&res)?);
                }
                MonitorKind::Tcp { host, port } => {
                    let res =
                        check_tcp(&host, port, 2000)
                            .await
                            .unwrap_or_else(|e| HealthCheckResult {
                                target: format!("{}:{}", host, port),
                                protocol: "tcp".to_string(),
                                timestamp: Utc::now(),
                                success: false,
                                latency_ms: None,
                                error: Some(e.to_string()),
                                observed_at: Utc::now(),
                                timeout_ms: 2000,
                                evidence: "tcp_probe".to_string(),
                            });
                    // Record metrics
                    let proto = res.protocol.as_str();
                    HEALTH_CHECK_TOTAL.with_label_values(&[proto]).inc();
                    if !res.success {
                        HEALTH_CHECK_FAILURES_TOTAL
                            .with_label_values(&[proto])
                            .inc();
                    }
                    if let Some(latency) = res.latency_ms {
                        let latency_seconds = latency as f64 / 1000.0;
                        HEALTH_CHECK_LATENCY_SECONDS
                            .with_label_values(&[proto])
                            .observe(latency_seconds);
                    }
                    println!("{}", to_string(&res)?);
                }
            }
            return Ok(());
        }
        Some(NetwatchCommand::Discover { kind }) => {
            match kind {
                DiscoverKind::Local => {
                    let interfaces = discover_interfaces().await.unwrap_or_default();
                    let routes = discover_routes().await.unwrap_or_default();
                    println!("Discovered {} interfaces:", interfaces.len());
                    for iface in interfaces {
                        println!(
                            "  - name: {}, state: {}, ip4: {}, ip6: {}, mac: {}, mtu: {}",
                            iface.name,
                            iface.oper_state.unwrap_or_else(|| "unknown".to_string()),
                            iface.ipv4.unwrap_or_else(|| "none".to_string()),
                            iface.ipv6.unwrap_or_else(|| "none".to_string()),
                            iface.mac.unwrap_or_else(|| "none".to_string()),
                            iface.mtu.unwrap_or(0)
                        );
                    }
                    println!("Discovered {} routes:", routes.len());
                    for rt in routes {
                        println!(
                            "  - dest: {}, via: {}, dev: {}, metric: {}, mask: {}",
                            rt.destination,
                            rt.gateway.unwrap_or_else(|| "none".to_string()),
                            rt.interface,
                            rt.metric.map_or("none".to_string(), |m| m.to_string()),
                            rt.mask.unwrap_or_else(|| "none".to_string())
                        );
                    }
                }
            }
            return Ok(());
        }
        None => {
            // Server mode handled below
        }
    }
    init_logging("info");
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await?;
    let app_metrics = AppMetrics::new();
    let uptime = app_metrics.uptime.clone();
    let start_time = app_metrics.start_time;
    let app = api_router().merge(Router::new().route(
        "/metrics",
        get(move || AppMetrics::metrics_handler(uptime.clone(), start_time)),
    ));
    println!("Running netwatch on {}", addr);
    serve(listener, app.into_make_service()).await?;
    Ok(())
}
