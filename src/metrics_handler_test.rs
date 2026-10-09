#[cfg(test)]
mod tests {
    use crate::metrics::AppMetrics;
    use crate::monitoring::metrics::{HEALTH_CHECK_LATENCY_SECONDS, HEALTH_CHECK_TOTAL};

    #[tokio::test]
    async fn test_metrics_handler_non_empty() {
        // Manually record metrics for a successful TCP check
        let proto = "tcp";
        HEALTH_CHECK_TOTAL.with_label_values(&[proto]).inc();
        HEALTH_CHECK_LATENCY_SECONDS
            .with_label_values(&[proto])
            .observe(0.1);
        // Create an AppMetrics instance to get uptime gauge and start_time
        let app_metrics = AppMetrics::new();
        let uptime = app_metrics.uptime.clone();
        let start_time = app_metrics.start_time;
        // Call the metrics handler
        let body = AppMetrics::metrics_handler(uptime, start_time).await;
        eprintln!("METRICS BODY:\n{}", body);
        assert!(!body.is_empty(), "metrics handler body should not be empty");
        // Should contain health check metrics
        assert!(body.contains("netwatch_health_checks_total"));
        assert!(body.contains("netwatch_health_check_latency_seconds"));
    }
}
