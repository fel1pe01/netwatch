use anyhow::Result;
use axum::serve;
use axum::{routing::get, Router};
use chrono::Utc;
use clap::{Parser, Subcommand};
use netwatch::api::router as api_router;
use netwatch::config::Config;
use netwatch::discovery::interfaces::discover_interfaces;
use netwatch::discovery::routing::discover_routes;
use netwatch::logging::init_logging;
use netwatch::metrics::AppMetrics;
use netwatch::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use netwatch::monitoring::{check_tcp, ping, HealthCheckResult};
use netwatch::scheduler::Scheduler;
use netwatch::storage::Storage;
use serde_json::to_string;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use uuid::Uuid;

/// Netwatch CLI
#[derive(Parser)]
#[command(name = "netwatch", version, about = "Netwatch network monitoring tool")]
struct Cli {
    #[arg(long, value_name = "PATH", help = "Path to configuration file")]
    config: Option<PathBuf>,
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

async fn register_monitoring_jobs(
    scheduler: &mut Scheduler,
    storage: Arc<Storage>,
    config: &Config,
) -> Result<()> {
    use netwatch::monitoring::metrics::{
        HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
    };
    use std::time::Duration;

    // For P0, we'll register a simple periodic check job that runs every health.interval seconds
    // In a real implementation, this would be driven by configured devices/services
    if config.health.interval > 0 {
        let storage = storage.clone();
        let interval = Duration::from_secs(config.health.interval);
        let job = netwatch::scheduler::Job {
            name: "health_check".to_string(),
            interval,
            task: Arc::new(move || {
                let storage = storage.clone();
                Box::pin(async move {
                    // Run a simple ICMP check to localhost as a placeholder
                    // In the future, this would iterate over configured devices/services
                    let res = ping("127.0.0.1", 2000)
                        .await
                        .unwrap_or_else(|e| HealthCheckResult {
                            target: "127.0.0.1".to_string(),
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

                    // Persist observation to database
                    if let Err(e) = persist_observation(&storage, &res).await {
                        tracing::error!("Failed to persist observation: {}", e);
                    }

                    Ok(())
                })
            }),
        };
        scheduler.add_job(job);
    }

    Ok(())
}

async fn persist_observation(storage: &Storage, result: &HealthCheckResult) -> Result<()> {
    use sqlx::query;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let device_id = "local"; // Placeholder - in real impl, would map target to device
    let metric = format!("{}_latency_ms", result.protocol);
    let value = result.latency_ms.unwrap_or(0) as f64;
    let source = result.evidence.clone();
    let confidence = if result.success { "high" } else { "low" }.to_string();

    query(
        r#"
        INSERT INTO observations (id, device_id, metric, value, source, confidence, observed_at, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(device_id)
    .bind(&metric)
    .bind(value)
    .bind(&source)
    .bind(&confidence)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&storage.pool)
    .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI
    let cli = Cli::parse();

    // Load configuration
    let config = if let Some(config_path) = cli.config {
        Config::from_path(&config_path)?
    } else {
        Config::default()
    };

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

    // Initialize logging from config
    init_logging(&config.logging.level);

    // Initialize storage
    let storage = Storage::connect(&config.storage.path).await?;
    let storage = Arc::new(storage);

    // Initialize scheduler
    let mut scheduler = Scheduler::new(config.scheduler.concurrency);

    // Register health check jobs from config
    register_monitoring_jobs(&mut scheduler, storage.clone(), &config).await?;

    // Start scheduler in background
    let scheduler_handle = tokio::spawn(async move {
        if let Err(e) = scheduler.run().await {
            tracing::error!("Scheduler error: {}", e);
        }
    });

    // Set up graceful shutdown
    let shutdown_signal = async move {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::info!("Shutdown signal received");
        // Give scheduler time to finish current jobs
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    };

    // Start HTTP server
    let addr: SocketAddr = config.agent.listen.parse()?;
    let listener = TcpListener::bind(addr).await?;
    let app_metrics = AppMetrics::new();
    let uptime = app_metrics.uptime.clone();
    let start_time = app_metrics.start_time;
    let app = api_router(storage).merge(Router::new().route(
        "/metrics",
        get(move || AppMetrics::metrics_handler(uptime.clone(), start_time)),
    ));

    println!("Running netwatch on {}", addr);

    // Run server with graceful shutdown
    tokio::select! {
        result = serve(listener, app.into_make_service()) => {
            result?;
        }
        _ = shutdown_signal => {
            tracing::info!("Shutting down gracefully...");
        }
    }

    // Shutdown scheduler
    scheduler_handle.abort();

    Ok(())
}
