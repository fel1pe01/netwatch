use prometheus::{Encoder, IntCounter, IntGauge, Opts, TextEncoder};
use std::time::Instant;

pub struct AppMetrics {
    pub info: IntGauge,
    pub uptime: IntGauge,
    pub jobs_total: IntCounter,
    pub errors_total: IntCounter,
    pub start_time: Instant,
}

impl AppMetrics {
    pub fn new() -> Self {
        let info_opts = Opts::new("netwatch_info", "netwatch instance info");
        let info = IntGauge::with_opts(info_opts).unwrap();
        let uptime_opts = Opts::new("netwatch_uptime_seconds", "uptime in seconds");
        let uptime = IntGauge::with_opts(uptime_opts).unwrap();
        let jobs_opts = Opts::new(
            "netwatch_scheduler_jobs_total",
            "Total scheduler jobs started",
        );
        let jobs_total = IntCounter::with_opts(jobs_opts).unwrap();
        let errors_opts = Opts::new("netwatch_scheduler_errors_total", "Total scheduler errors");
        let errors_total = IntCounter::with_opts(errors_opts).unwrap();

        let start_time = Instant::now();

        let registry = prometheus::default_registry();
        let _ = registry.register(Box::new(info.clone()));
        let _ = registry.register(Box::new(uptime.clone()));
        let _ = registry.register(Box::new(jobs_total.clone()));
        let _ = registry.register(Box::new(errors_total.clone()));

        // Set initial uptime to 0
        uptime.set(0);

        Self {
            info,
            uptime,
            jobs_total,
            errors_total,
            start_time,
        }
    }

    pub async fn metrics_handler(uptime: IntGauge, start_time: Instant) -> String {
        // Update uptime before gathering metrics
        let elapsed = start_time.elapsed().as_secs() as i64;
        uptime.set(elapsed);

        let registry = prometheus::default_registry();
        let encoder = TextEncoder::new();
        let metric_families = registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}

impl Default for AppMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for AppMetrics {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            uptime: self.uptime.clone(),
            jobs_total: self.jobs_total.clone(),
            errors_total: self.errors_total.clone(),
            start_time: self.start_time,
        }
    }
}
