use once_cell::sync::Lazy;
use prometheus::{register_counter_vec, register_histogram_vec, CounterVec, HistogramVec};

pub static HEALTH_CHECK_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "netwatch_health_checks_total",
        "Total number of health checks performed",
        &["protocol"]
    )
    .expect("failed to create counter")
});

pub static HEALTH_CHECK_FAILURES_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "netwatch_health_check_failures_total",
        "Total number of health check failures",
        &["protocol"]
    )
    .expect("failed to create counter")
});

pub static HEALTH_CHECK_LATENCY_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "netwatch_health_check_latency_seconds",
        "Latency of health checks in seconds",
        &["protocol"]
    )
    .expect("failed to create histogram")
});

/// Synthetic check per-phase latency metrics
pub static SYNTHETIC_CHECK_DNS_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "netwatch_synthetic_check_dns_duration_seconds",
        "DNS resolution duration for synthetic checks in seconds",
        &["target"]
    )
    .expect("failed to create histogram")
});

pub static SYNTHETIC_CHECK_TCP_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "netwatch_synthetic_check_tcp_duration_seconds",
        "TCP connection duration for synthetic checks in seconds",
        &["target"]
    )
    .expect("failed to create histogram")
});

pub static SYNTHETIC_CHECK_TLS_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "netwatch_synthetic_check_tls_duration_seconds",
        "TLS handshake duration for synthetic checks in seconds",
        &["target"]
    )
    .expect("failed to create histogram")
});

pub static SYNTHETIC_CHECK_TTFB_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "netwatch_synthetic_check_ttfb_duration_seconds",
        "Time to first byte for synthetic checks in seconds",
        &["target"]
    )
    .expect("failed to create histogram")
});

pub static SYNTHETIC_CHECK_TOTAL_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "netwatch_synthetic_check_total_duration_seconds",
        "Total duration for synthetic checks in seconds",
        &["target"]
    )
    .expect("failed to create histogram")
});

pub static SYNTHETIC_CHECK_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "netwatch_synthetic_checks_total",
        "Total number of synthetic checks performed",
        &["protocol", "result"]
    )
    .expect("failed to create counter")
});

pub static NETWATCH_SCHEDULER_JOBS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "netwatch_scheduler_jobs_total",
        "Total number of scheduler jobs started",
        &["job"]
    )
    .expect("failed to create counter")
});

pub static NETWATCH_SCHEDULER_ERRORS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "netwatch_scheduler_errors_total",
        "Total number of scheduler job errors",
        &["job"]
    )
    .expect("failed to create counter")
});
