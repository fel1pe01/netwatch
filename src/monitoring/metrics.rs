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
