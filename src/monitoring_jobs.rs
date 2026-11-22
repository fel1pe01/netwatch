use crate::config::{Config, SyntheticCheckType};
use crate::discovery::source::{DiscoverySource, SystemDiscoverySource};
use crate::events::{CheckEvidence, EntityType, EventBus};
use crate::monitoring::metrics::{
    HEALTH_CHECK_FAILURES_TOTAL, HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL,
    SYNTHETIC_CHECK_DNS_DURATION_SECONDS, SYNTHETIC_CHECK_TCP_DURATION_SECONDS,
    SYNTHETIC_CHECK_TLS_DURATION_SECONDS, SYNTHETIC_CHECK_TOTAL,
    SYNTHETIC_CHECK_TOTAL_DURATION_SECONDS, SYNTHETIC_CHECK_TTFB_DURATION_SECONDS,
};
use crate::monitoring::validation::validation_error_result;
use crate::monitoring::HttpPhaseTimings;
use crate::scheduler::{Job, Scheduler};
use crate::ssrf::SsrfPolicy;
use crate::state_tracker::{StateTracker, StateTrackerConfig};
use crate::storage::{
    load_last_check_state, CheckResult, CheckResultMessage, Storage, UpsertInterfaceMessage,
    UpsertRouteMessage,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Register monitoring jobs from configuration
pub async fn register_monitoring_jobs(
    scheduler: &mut Scheduler,
    storage: Arc<Storage>,
    event_bus: Arc<EventBus>,
    check_result_tx: mpsc::Sender<CheckResultMessage>,
    config: &Config,
    ssrf_policy: Arc<SsrfPolicy>,
) -> Result<()> {
    // Create state tracker with config
    let tracker_config = StateTrackerConfig {
        consecutive_failures_threshold: config
            .scheduler
            .consecutive_failures_threshold
            .unwrap_or(3),
        consecutive_successes_threshold: config
            .scheduler
            .consecutive_successes_threshold
            .unwrap_or(1),
    };
    let state_tracker = Arc::new(tokio::sync::Mutex::new(StateTracker::new(tracker_config)));

    // Register synthetic checks from config
    if config.synthetic_checks.enabled.unwrap_or(true) {
        let interval = Duration::from_secs(config.synthetic_checks.interval.unwrap_or(60));

        for check in &config.synthetic_checks.checks {
            if !check.enabled {
                continue;
            }

            let storage = storage.clone();
            let event_bus = event_bus.clone();
            let check_result_tx = check_result_tx.clone();
            let state_tracker = state_tracker.clone();
            let check_config = check.clone();
            let policy = ssrf_policy.clone();
            let job_name = format!("synthetic_check_{}", check.id);
            let entity_id = check.id.clone();
            let entity_type = EntityType::Service;
            let entity_type_str = format!("{:?}", entity_type);

            // Load initial state from DB into state tracker
            let initial_state =
                load_last_check_state(&storage.pool, &entity_id, &entity_type_str).await?;
            if let Some((last_success, consecutive_failures, consecutive_successes)) = initial_state
            {
                state_tracker
                    .lock()
                    .await
                    .load_state(crate::state_tracker::EntityState::from_db(
                        entity_id.clone(),
                        entity_type_str.clone(),
                        Some(last_success),
                        consecutive_failures,
                        consecutive_successes,
                    ));
            }

            let job = Job {
                name: job_name.clone(),
                interval,
                task: Arc::new(move || {
                    let event_bus = event_bus.clone();
                    let check_result_tx = check_result_tx.clone();
                    let state_tracker = state_tracker.clone();
                    let check = check_config.clone();
                    let policy = policy.clone();
                    let entity_id = entity_id.clone();
                    let entity_type = entity_type.clone();
                    let entity_type_str = entity_type_str.clone();
                    Box::pin(async move {
                        let (res, proto, phase_timings) = match check.check_type {
                            SyntheticCheckType::Http | SyntheticCheckType::Https => {
                                let url = if check.check_type == SyntheticCheckType::Https {
                                    format!("https://{}", check.target)
                                } else {
                                    format!("http://{}", check.target)
                                };
                                let timeout = check.timeout_ms.unwrap_or(5000);
                                let expected_status = check.expected_status;
                                let expected_body = check.expected_body.clone();
                                let headers = check.headers.clone().unwrap_or_default();
                                let proto_str = if check.check_type == SyntheticCheckType::Https {
                                    "https"
                                } else {
                                    "http"
                                };
                                let (res, phases) =
                                    crate::monitoring::http::check_http_with_phases_ssrf(
                                        &policy,
                                        &url,
                                        timeout,
                                        expected_status,
                                        expected_body,
                                        Some(headers),
                                    )
                                    .await
                                    .unwrap_or_else(|e| {
                                        (
                                            validation_error_result(&url, proto_str, timeout, e),
                                            HttpPhaseTimings::default(),
                                        )
                                    });
                                (res, proto_str.to_string(), Some(phases))
                            }
                            SyntheticCheckType::Dns => {
                                let timeout = check.timeout_ms.unwrap_or(5000);
                                let record_type = check.record_type.clone();
                                let res = crate::monitoring::dns::check_dns_with_ssrf(
                                    &policy,
                                    &check.target,
                                    record_type,
                                    None,
                                    timeout,
                                )
                                .await
                                .unwrap_or_else(|e| {
                                    validation_error_result(&check.target, "dns", timeout, e)
                                });
                                (res, "dns".to_string(), None)
                            }
                            SyntheticCheckType::Tcp => {
                                let port = check.port.unwrap_or(80);
                                let timeout = check.timeout_ms.unwrap_or(5000);
                                let res = crate::monitoring::tcp::check_tcp_with_ssrf(
                                    &policy,
                                    &check.target,
                                    port,
                                    timeout,
                                )
                                .await
                                .unwrap_or_else(|e| {
                                    validation_error_result(
                                        &format!("{}:{}", check.target, port),
                                        "tcp",
                                        timeout,
                                        e,
                                    )
                                });
                                (res, "tcp".to_string(), None)
                            }
                            SyntheticCheckType::Tls => {
                                let port = check.port.unwrap_or(443);
                                let timeout = check.timeout_ms.unwrap_or(5000);
                                let check_expiry_days = check.check_expiry_days;
                                let verify_hostname = check.verify_hostname.unwrap_or(true);
                                let res = crate::monitoring::tls::check_tls_with_ssrf(
                                    &policy,
                                    &check.target,
                                    port,
                                    timeout,
                                    check_expiry_days,
                                    verify_hostname,
                                )
                                .await
                                .unwrap_or_else(|e| {
                                    validation_error_result(
                                        &format!("{}:{}", check.target, port),
                                        "tls",
                                        timeout,
                                        e,
                                    )
                                });
                                (res, "tls".to_string(), None)
                            }
                        };

                        // Record synthetic check metrics
                        let result_label = if res.success { "success" } else { "failure" };
                        SYNTHETIC_CHECK_TOTAL
                            .with_label_values(&[&proto, result_label])
                            .inc();
                        if let Some(latency) = res.latency_ms {
                            let latency_seconds = latency as f64 / 1000.0;
                            SYNTHETIC_CHECK_TOTAL_DURATION_SECONDS
                                .with_label_values(&[&check.target])
                                .observe(latency_seconds);
                        }
                        // Record per-phase metrics if available
                        if let Some(phases) = phase_timings {
                            if let Some(dns_ms) = phases.dns_ms {
                                SYNTHETIC_CHECK_DNS_DURATION_SECONDS
                                    .with_label_values(&[&check.target])
                                    .observe(dns_ms as f64 / 1000.0);
                            }
                            if let Some(tcp_ms) = phases.tcp_ms {
                                SYNTHETIC_CHECK_TCP_DURATION_SECONDS
                                    .with_label_values(&[&check.target])
                                    .observe(tcp_ms as f64 / 1000.0);
                            }
                            if let Some(tls_ms) = phases.tls_ms {
                                SYNTHETIC_CHECK_TLS_DURATION_SECONDS
                                    .with_label_values(&[&check.target])
                                    .observe(tls_ms as f64 / 1000.0);
                            }
                            if let Some(ttfb_ms) = phases.ttfb_ms {
                                SYNTHETIC_CHECK_TTFB_DURATION_SECONDS
                                    .with_label_values(&[&check.target])
                                    .observe(ttfb_ms as f64 / 1000.0);
                            }
                        }

                        // Process through state tracker for hysteresis
                        let check_evidence = CheckEvidence {
                            target: res.target.clone(),
                            protocol: res.protocol.clone(),
                            success: res.success,
                            latency_ms: res.latency_ms,
                            error: res.error.clone(),
                        };
                        let transition = state_tracker.lock().await.process_check(
                            check_evidence.clone(),
                            &entity_id,
                            &entity_type_str,
                        );

                        // Persist check result via actor
                        let check_result = CheckResult {
                            id: Uuid::new_v4().to_string(),
                            entity_id: entity_id.clone(),
                            entity_type: entity_type_str.clone(),
                            target: res.target.clone(),
                            protocol: res.protocol.clone(),
                            success: res.success,
                            latency_ms: res.latency_ms,
                            error: res.error.clone(),
                            consecutive_failures: transition.consecutive_failures,
                            consecutive_successes: transition.consecutive_successes,
                            last_state_change: transition.should_emit_event,
                            previous_state: transition.previous_state,
                            observed_at: res.observed_at,
                            created_at: Utc::now(),
                        };
                        if let Err(e) = check_result_tx
                            .send(CheckResultMessage::WriteCheckResult(check_result))
                            .await
                        {
                            tracing::error!("Failed to send check result to writer: {}", e);
                        }

                        // Emit event ONLY when hysteresis threshold crossed
                        if transition.should_emit_event {
                            if let Err(e) = event_bus
                                .emit_check_state_change(
                                    entity_id.clone(),
                                    entity_type.clone(),
                                    check_evidence,
                                    transition.previous_state,
                                )
                                .await
                            {
                                tracing::error!("Failed to emit check state change event: {}", e);
                            }
                        }

                        Ok(())
                    })
                }),
            };
            scheduler.add_job(job);
        }
    }

    // Backward compatibility: register legacy health check jobs if no synthetic checks configured
    let has_synthetic_checks = !config.synthetic_checks.checks.is_empty();

    if !has_synthetic_checks && config.health.interval > 0 {
        let interval = Duration::from_secs(config.health.interval);

        let event_bus = event_bus.clone();
        let check_result_tx = check_result_tx.clone();
        let state_tracker = state_tracker.clone();
        let policy = ssrf_policy.clone(); // Use MAIN policy (not internal)
        let entity_id = "local".to_string();
        let entity_type = EntityType::Device;
        let entity_type_str = format!("{:?}", entity_type);

        // Load initial state from DB into state tracker
        let initial_state =
            load_last_check_state(&storage.pool, &entity_id, &entity_type_str).await?;
        if let Some((last_success, consecutive_failures, consecutive_successes)) = initial_state {
            state_tracker
                .lock()
                .await
                .load_state(crate::state_tracker::EntityState::from_db(
                    entity_id.clone(),
                    entity_type_str.clone(),
                    Some(last_success),
                    consecutive_failures,
                    consecutive_successes,
                ));
        }

        let job = Job {
            name: "health_check_icmp".to_string(),
            interval,
            task: Arc::new(move || {
                let event_bus = event_bus.clone();
                let check_result_tx = check_result_tx.clone();
                let state_tracker = state_tracker.clone();
                let policy = policy.clone();
                let entity_id = entity_id.clone();
                let entity_type = entity_type.clone();
                let entity_type_str = entity_type_str.clone();
                Box::pin(async move {
                    let res = crate::monitoring::icmp::ping_with_ssrf(&policy, "127.0.0.1", 2000)
                        .await
                        .unwrap_or_else(|e| validation_error_result("127.0.0.1", "icmp", 2000, e));

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

                    // Process through state tracker for hysteresis
                    let check_evidence = CheckEvidence {
                        target: res.target.clone(),
                        protocol: res.protocol.clone(),
                        success: res.success,
                        latency_ms: res.latency_ms,
                        error: res.error.clone(),
                    };
                    let transition = state_tracker.lock().await.process_check(
                        check_evidence.clone(),
                        &entity_id,
                        &entity_type_str,
                    );

                    // Persist check result via actor
                    let check_result = CheckResult {
                        id: Uuid::new_v4().to_string(),
                        entity_id: entity_id.clone(),
                        entity_type: entity_type_str.clone(),
                        target: res.target.clone(),
                        protocol: res.protocol.clone(),
                        success: res.success,
                        latency_ms: res.latency_ms,
                        error: res.error.clone(),
                        consecutive_failures: transition.consecutive_failures,
                        consecutive_successes: transition.consecutive_successes,
                        last_state_change: transition.should_emit_event,
                        previous_state: transition.previous_state,
                        observed_at: res.observed_at,
                        created_at: Utc::now(),
                    };
                    if let Err(e) = check_result_tx
                        .send(CheckResultMessage::WriteCheckResult(check_result))
                        .await
                    {
                        tracing::error!("Failed to send check result to writer: {}", e);
                    }

                    // Emit event ONLY when hysteresis threshold crossed
                    if transition.should_emit_event {
                        if let Err(e) = event_bus
                            .emit_check_state_change(
                                entity_id.clone(),
                                entity_type.clone(),
                                check_evidence,
                                transition.previous_state,
                            )
                            .await
                        {
                            tracing::error!("Failed to emit check state change event: {}", e);
                        }
                    }

                    Ok(())
                })
            }),
        };
        scheduler.add_job(job);
    }

    // Register discovery persistence job (runs on startup and then periodically)
    if config.discovery.enabled && config.discovery.interval > 0 {
        let interval = Duration::from_secs(config.discovery.interval);
        let storage = storage.clone();
        let check_result_tx = check_result_tx.clone();
        let discovery_source = Arc::new(SystemDiscoverySource);

        let task = create_discovery_persist_task(storage, check_result_tx, discovery_source);

        let job = Job {
            name: "discovery_persist".to_string(),
            interval,
            task,
        };
        scheduler.add_job(job);
    }

    Ok(())
}

// Discovery persist task struct
struct DiscoveryPersistTask {
    storage: Arc<Storage>,
    check_result_tx: mpsc::Sender<CheckResultMessage>,
    discovery_source: Arc<dyn DiscoverySource>,
}

impl DiscoveryPersistTask {
    fn run(&self) -> BoxFuture<'static, Result<()>> {
        let storage = self.storage.clone();
        let check_result_tx = self.check_result_tx.clone();
        let discovery_source = self.discovery_source.clone();
        let now = Utc::now();
        Box::pin(async move {
            // Run discovery in spawn_blocking (blocking I/O)
            let interfaces_result = tokio::task::spawn_blocking({
                let discovery_source = discovery_source.clone();
                move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(discovery_source.discover_interfaces())
                }
            })
            .await;

            let routes_result = tokio::task::spawn_blocking({
                let discovery_source = discovery_source.clone();
                move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(discovery_source.discover_routes())
                }
            })
            .await;

            let now = now;
            let now_str = now.to_rfc3339();

            match interfaces_result {
                Ok(Ok(interfaces)) => {
                    for iface in interfaces {
                        let device_id = iface.ipv4.clone().unwrap_or_else(|| iface.name.clone());
                        let device_type =
                            if iface.name.starts_with("veth") || iface.name.starts_with("docker") {
                                "virtual"
                            } else if iface.name == "lo" {
                                "loopback"
                            } else {
                                "physical"
                            };

                        if let Err(e) = upsert_device(
                            &storage,
                            &device_id,
                            &iface.name,
                            &iface.ipv4,
                            &iface.ipv6,
                            &iface.mac,
                            device_type,
                            &now,
                        )
                        .await
                        {
                            tracing::error!("Failed to upsert device {}: {}", device_id, e);
                        }

                        // Send interface upsert through writer actor
                        let interface_msg = UpsertInterfaceMessage {
                            id: iface.name.clone(),
                            device_id: device_id.clone(),
                            name: iface.name.clone(),
                            mac: iface.mac.clone(),
                            mtu: iface.mtu.map(|v| v as i64),
                            state: iface.oper_state.clone(),
                            source: "discovery".to_string(),
                            confidence: "high".to_string(),
                            observed_at: iface.oper_state.clone(),
                            created_at: now_str.clone(),
                            updated_at: now_str.clone(),
                            last_seen: now_str.clone(),
                        };
                        if let Err(e) = check_result_tx
                            .send(CheckResultMessage::UpsertInterface(interface_msg))
                            .await
                        {
                            tracing::error!("Failed to send interface upsert: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Interface discovery failed: {}", e);
                }
                Err(e) => {
                    tracing::error!("Interface discovery task failed: {}", e);
                }
            }

            match routes_result {
                Ok(Ok(routes)) => {
                    for route in routes {
                        // Send route upsert through writer actor
                        let route_msg = UpsertRouteMessage {
                            id: format!("{}|{}", route.destination, route.interface),
                            destination: route.destination.clone(),
                            gateway: route.gateway.clone(),
                            interface: route.interface.clone(),
                            metric: route.metric.map(|v| v as i64),
                            mask: route.mask.clone(),
                            family: "ipv4".to_string(),
                            last_seen: now_str.clone(),
                            created_at: now_str.clone(),
                            updated_at: now_str.clone(),
                        };
                        if let Err(e) = check_result_tx
                            .send(CheckResultMessage::UpsertRoute(route_msg))
                            .await
                        {
                            tracing::error!("Failed to send route upsert: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Route discovery failed: {}", e);
                }
                Err(e) => {
                    tracing::error!("Route discovery task failed: {}", e);
                }
            }

            Ok(())
        })
    }
}

/// Create the discovery persist task
pub fn create_discovery_persist_task(
    storage: Arc<Storage>,
    check_result_tx: mpsc::Sender<CheckResultMessage>,
    discovery_source: Arc<dyn DiscoverySource>,
) -> Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync> {
    let task = DiscoveryPersistTask {
        storage,
        check_result_tx,
        discovery_source,
    };
    Arc::new(move || task.run())
}
/// Upsert device with last_seen
#[allow(clippy::too_many_arguments, unused_variables)]
async fn upsert_device(
    storage: &Storage,
    device_id: &str,
    hostname: &str,
    ipv4: &Option<String>,
    _ipv6: &Option<String>,
    mac: &Option<String>,
    device_type: &str,
    now: &DateTime<Utc>,
) -> Result<()> {
    let ip = ipv4.as_deref().unwrap_or("");
    let mac = mac.as_deref().unwrap_or("");
    let now_str = now.to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at, last_seen)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            hostname = excluded.hostname,
            ip = excluded.ip,
            mac = excluded.mac,
            device_type = excluded.device_type,
            source = excluded.source,
            confidence = excluded.confidence,
            observed_at = excluded.observed_at,
            updated_at = excluded.updated_at,
            last_seen = excluded.last_seen
        "#,
    )
    .bind(device_id)
    .bind(hostname)
    .bind(ip)
    .bind(mac)
    .bind(device_type)
    .bind("discovery")
    .bind("high")
    .bind(&now_str)
    .bind(&now_str)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&storage.pool)
    .await?;

    Ok(())
}
