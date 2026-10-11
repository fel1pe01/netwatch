use anyhow::Result;
use axum::serve;
use axum::{routing::get, Router};
use chrono::Utc;
use clap::{Parser, Subcommand};
use netwatch::api::router as api_router;
use netwatch::config::Config;
use netwatch::discovery::interfaces::discover_interfaces;
use netwatch::discovery::routing::discover_routes;
use netwatch::events::{CheckEvidence, EntityType, EventBus};
use netwatch::logging::init_logging;
use netwatch::metrics::AppMetrics;
use netwatch::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
};
use netwatch::monitoring::{check_tcp, ping, HealthCheckResult};
use netwatch::scheduler::Scheduler;
use netwatch::snmp::SnmpCollector;
use netwatch::storage::Storage;
use netwatch::topology_builder::TopologyBuilder;
use netwatch::topology_store::TopologyStore;
use serde_json::to_string;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
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
    event_bus: Arc<EventBus>,
    config: &Config,
) -> Result<()> {
    use netwatch::monitoring::metrics::{
        HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
    };
    use netwatch::monitoring::{check_dns, check_http, check_tls, ping};
    use std::time::Duration;

    // ICMP health check job (P0)
    if config.health.interval > 0 {
        let storage = storage.clone();
        let event_bus = event_bus.clone();
        let interval = Duration::from_secs(config.health.interval);
        let job = netwatch::scheduler::Job {
            name: "health_check_icmp".to_string(),
            interval,
            task: Arc::new(move || {
                let storage = storage.clone();
                let event_bus = event_bus.clone();
                Box::pin(async move {
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

                    // Emit check completed/failed event
                    let check_evidence = CheckEvidence {
                        target: res.target.clone(),
                        protocol: res.protocol.clone(),
                        success: res.success,
                        latency_ms: res.latency_ms,
                        error: res.error.clone(),
                    };
                    if let Err(e) = event_bus
                        .emit_check_completed(
                            "local".to_string(),
                            EntityType::Device,
                            check_evidence,
                        )
                        .await
                    {
                        tracing::error!("Failed to emit check event: {}", e);
                    }

                    Ok(())
                })
            }),
        };
        scheduler.add_job(job);
    }

    // HTTP health check job (P1)
    if config.health.interval > 0 {
        let storage = storage.clone();
        let event_bus = event_bus.clone();
        let interval = Duration::from_secs(config.health.interval);
        let job = netwatch::scheduler::Job {
            name: "health_check_http".to_string(),
            interval,
            task: Arc::new(move || {
                let storage = storage.clone();
                let event_bus = event_bus.clone();
                Box::pin(async move {
                    let res =
                        check_http("http://127.0.0.1:8080/health", 2000, Some(200), None, None)
                            .await
                            .unwrap_or_else(|e| HealthCheckResult {
                                target: "http://127.0.0.1:8080/health".to_string(),
                                protocol: "http".to_string(),
                                timestamp: Utc::now(),
                                success: false,
                                latency_ms: None,
                                error: Some(e.to_string()),
                                observed_at: Utc::now(),
                                timeout_ms: 2000,
                                evidence: "http_probe".to_string(),
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

                    // Emit check completed/failed event
                    let check_evidence = CheckEvidence {
                        target: res.target.clone(),
                        protocol: res.protocol.clone(),
                        success: res.success,
                        latency_ms: res.latency_ms,
                        error: res.error.clone(),
                    };
                    if let Err(e) = event_bus
                        .emit_check_completed(
                            "local".to_string(),
                            EntityType::Device,
                            check_evidence,
                        )
                        .await
                    {
                        tracing::error!("Failed to emit check event: {}", e);
                    }

                    Ok(())
                })
            }),
        };
        scheduler.add_job(job);
    }

    // DNS health check job (P1)
    if config.health.interval > 0 {
        let storage = storage.clone();
        let event_bus = event_bus.clone();
        let interval = Duration::from_secs(config.health.interval);
        let job = netwatch::scheduler::Job {
            name: "health_check_dns".to_string(),
            interval,
            task: Arc::new(move || {
                let storage = storage.clone();
                let event_bus = event_bus.clone();
                Box::pin(async move {
                    let res = check_dns("localhost", Some("A".to_string()), None, 2000)
                        .await
                        .unwrap_or_else(|e| HealthCheckResult {
                            target: "localhost (A)".to_string(),
                            protocol: "dns".to_string(),
                            timestamp: Utc::now(),
                            success: false,
                            latency_ms: None,
                            error: Some(e.to_string()),
                            observed_at: Utc::now(),
                            timeout_ms: 2000,
                            evidence: "dns_probe".to_string(),
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

                    // Emit check completed/failed event
                    let check_evidence = CheckEvidence {
                        target: res.target.clone(),
                        protocol: res.protocol.clone(),
                        success: res.success,
                        latency_ms: res.latency_ms,
                        error: res.error.clone(),
                    };
                    if let Err(e) = event_bus
                        .emit_check_completed(
                            "local".to_string(),
                            EntityType::Device,
                            check_evidence,
                        )
                        .await
                    {
                        tracing::error!("Failed to emit check event: {}", e);
                    }

                    Ok(())
                })
            }),
        };
        scheduler.add_job(job);
    }

    // TLS health check job (P1)
    if config.health.interval > 0 {
        let storage = storage.clone();
        let event_bus = event_bus.clone();
        let interval = Duration::from_secs(config.health.interval);
        let job = netwatch::scheduler::Job {
            name: "health_check_tls".to_string(),
            interval,
            task: Arc::new(move || {
                let storage = storage.clone();
                let event_bus = event_bus.clone();
                Box::pin(async move {
                    let res = check_tls("localhost", 8080, 2000, Some(30), false)
                        .await
                        .unwrap_or_else(|e| HealthCheckResult {
                            target: "localhost:8080".to_string(),
                            protocol: "tls".to_string(),
                            timestamp: Utc::now(),
                            success: false,
                            latency_ms: None,
                            error: Some(e.to_string()),
                            observed_at: Utc::now(),
                            timeout_ms: 2000,
                            evidence: "tls_probe".to_string(),
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

                    // Emit check completed/failed event
                    let check_evidence = CheckEvidence {
                        target: res.target.clone(),
                        protocol: res.protocol.clone(),
                        success: res.success,
                        latency_ms: res.latency_ms,
                        error: res.error.clone(),
                    };
                    if let Err(e) = event_bus
                        .emit_check_completed(
                            "local".to_string(),
                            EntityType::Device,
                            check_evidence,
                        )
                        .await
                    {
                        tracing::error!("Failed to emit check event: {}", e);
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

    // Initialize topology store
    let topology_store = TopologyStore::new(storage.pool.clone());
    let topology_store = Arc::new(topology_store);

    // Initialize topology builder
    let topology_builder = TopologyBuilder::new(topology_store.clone());

    // Initialize SNMP collector
    let snmp_collector = SnmpCollector::new(config.scheduler.concurrency, 100);

    // Initialize event bus
    let event_bus = Arc::new(EventBus::new());

    // Initialize scheduler
    let mut scheduler = Scheduler::new(config.scheduler.concurrency);

    // Register health check jobs from config
    register_monitoring_jobs(&mut scheduler, storage.clone(), event_bus, &config).await?;

    // Register topology discovery job
    if config.discovery.enabled && config.discovery.interval > 0 {
        let topology_builder = topology_builder.clone();
        let interval = Duration::from_secs(config.discovery.interval);
        let job = netwatch::scheduler::Job {
            name: "topology_discovery".to_string(),
            interval,
            task: {
                let builder = topology_builder.clone();
                Arc::new(move || {
                    let mut builder = builder.clone();
                    Box::pin(async move {
                        if let Err(e) = builder.build_from_discovery().await {
                            tracing::error!("Topology discovery failed: {}", e);
                        }
                        if let Err(e) = builder.create_snapshot().await {
                            tracing::error!("Failed to create topology snapshot: {}", e);
                        }
                        Ok(())
                    })
                })
            },
        };
        scheduler.add_job(job);
    }

    // Register SNMP polling jobs
    if config.snmp.enabled && config.snmp.interval > 0 {
        let snmp_collector = Arc::new(snmp_collector);
        let storage = storage.clone();
        let interval = Duration::from_secs(config.snmp.interval);
        let job = netwatch::scheduler::Job {
            name: "snmp_poll".to_string(),
            interval,
            task: Arc::new(move || {
                let _collector = snmp_collector.clone();
                let _storage = storage.clone();
                Box::pin(async move {
                    // In a real implementation, this would iterate over configured SNMP targets
                    // For now, this is a placeholder
                    tracing::debug!("SNMP polling job executed (placeholder)");
                    Ok(())
                })
            }),
        };
        scheduler.add_job(job);
    }

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
