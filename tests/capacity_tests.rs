use anyhow::Result;
use chrono::{Duration, Utc};
use netwatch::capacity_planning::{CapacityConfig, CapacityPlanner};
use netwatch::storage::Storage;
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

async fn setup_test_storage(dir: &tempfile::TempDir) -> Arc<Storage> {
    let db_path = dir.path().join("test.sqlite");
    let storage = Storage::connect(db_path.to_str().unwrap()).await.unwrap();
    Arc::new(storage)
}

async fn ensure_device_exists(storage: &Storage, device_id: &str) {
    let row = sqlx::query("SELECT id FROM devices WHERE id = ?")
        .bind(device_id)
        .fetch_optional(&storage.pool)
        .await
        .unwrap();

    if row.is_none() {
        sqlx::query(
            r#"INSERT INTO devices (id, hostname, ip, mac, device_type, source, confidence, observed_at, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
        )
        .bind(device_id)
        .bind(device_id)
        .bind("127.0.0.1")
        .bind("00:00:00:00:00:00")
        .bind("test")
        .bind("test")
        .bind("high")
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&storage.pool)
        .await
        .unwrap();
    }
}

async fn insert_latency_observations(
    storage: &Storage,
    device_id: &str,
    protocol: &str,
    values: &[f64],
    base_time: chrono::DateTime<Utc>,
) {
    ensure_device_exists(storage, device_id).await;
    for (i, value) in values.iter().enumerate() {
        let obs = netwatch::storage::Observation {
            id: Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            metric: format!("{}_latency_ms", protocol),
            value: *value,
            source: "test".to_string(),
            confidence: "high".to_string(),
            observed_at: base_time + Duration::seconds(i as i64 * 3600), // 1 hour intervals
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        storage.insert_observation(&obs).await.unwrap();
    }
}

#[tokio::test]
async fn test_linear_regression_increasing_regular_intervals() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0), // 1000ms nominal capacity
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let now = Utc::now();
    // Insert 30 observations at 1-day intervals over 30 days
    ensure_device_exists(&storage, "test-device").await;
    for i in 0..30 {
        let obs = netwatch::storage::Observation {
            id: Uuid::new_v4().to_string(),
            device_id: "test-device".to_string(),
            metric: "icmp_latency_ms".to_string(),
            value: 100.0 + i as f64,
            source: "test".to_string(),
            confidence: "high".to_string(),
            observed_at: now - Duration::days(30 - i as i64),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        storage.insert_observation(&obs).await.unwrap();
    }

    let report = planner.analyze_capacity().await?;

    assert_eq!(report.trends.len(), 1);
    let trend = &report.trends[0];
    assert!(trend.slope > 0.0); // Positive slope
    assert!(trend.r_squared > 0.95); // Strong linear fit
                                     // Sample count may vary slightly due to window boundaries; accept 29-30
    assert!(trend.sample_count >= 29 && trend.sample_count <= 30);
    assert_eq!(trend.capacity_source, "nominal_capacity");
    assert_eq!(trend.saturation_threshold, 800.0); // 80% of 1000ms

    // Time to saturation should be projected
    assert!(trend.time_to_saturation.is_some());
    Ok(())
}

#[tokio::test]
async fn test_linear_regression_increasing_irregular_intervals() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    // Irregular intervals but still linear trend
    let values: Vec<f64> = vec![
        100.0, 101.0, 102.5, 103.0, 104.0, 105.5, 106.0, 107.0, 108.5, 109.0, 110.0, 111.5, 112.0,
        113.0, 114.5, 115.0, 116.0, 117.5, 118.0, 119.0, 120.0, 121.5, 122.0, 123.0, 124.5, 125.0,
        126.0, 127.5, 128.0, 129.0,
    ];
    // Insert with irregular timestamps (varying intervals)
    ensure_device_exists(&storage, "test-device").await;
    for (i, value) in values.iter().enumerate() {
        let mut cumulative_hours = 0;
        for j in 0..=i {
            cumulative_hours += match j % 4 {
                0 => 24,
                1 => 12,
                2 => 36,
                _ => 48,
            };
        }
        let obs = netwatch::storage::Observation {
            id: Uuid::new_v4().to_string(),
            device_id: "test-device".to_string(),
            metric: "icmp_latency_ms".to_string(),
            value: *value,
            source: "test".to_string(),
            confidence: "high".to_string(),
            observed_at: base_time + Duration::hours(cumulative_hours as i64),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        storage.insert_observation(&obs).await.unwrap();
    }

    let report = planner.analyze_capacity().await?;

    assert_eq!(report.trends.len(), 1);
    let trend = &report.trends[0];
    assert!(trend.slope > 0.0);
    assert!(trend.r_squared > 0.85); // Good fit despite irregular intervals
    Ok(())
}

#[tokio::test]
async fn test_linear_regression_decreasing_series() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    // Decreasing trend (improving latency)
    let values: Vec<f64> = (0..30).map(|i| 200.0 - i as f64 * 2.0).collect();
    insert_latency_observations(&storage, "test-device", "icmp", &values, base_time).await;

    let report = planner.analyze_capacity().await?;

    assert_eq!(report.trends.len(), 1);
    let trend = &report.trends[0];
    assert!(trend.slope < 0.0); // Negative slope
    assert!(trend.r_squared > 0.95);
    // No time to saturation since slope is negative
    assert!(trend.time_to_saturation.is_none());
    Ok(())
}

#[tokio::test]
async fn test_constant_series_no_saturation() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    // Constant latency
    let values: Vec<f64> = vec![100.0; 30];
    insert_latency_observations(&storage, "test-device", "icmp", &values, base_time).await;

    let report = planner.analyze_capacity().await?;

    assert_eq!(report.trends.len(), 1);
    let trend = &report.trends[0];
    assert_eq!(trend.slope, 0.0);
    assert_eq!(trend.r_squared, 1.0); // Perfect fit for constant
    assert!(trend.time_to_saturation.is_none());
    Ok(())
}

#[tokio::test]
async fn test_insufficient_data_warning() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    // Only 5 samples (< min_samples of 10)
    let values: Vec<f64> = vec![100.0; 5];
    insert_latency_observations(&storage, "test-device", "icmp", &values, base_time).await;

    let report = planner.analyze_capacity().await?;

    // Should be in insufficient_data, not trends
    assert_eq!(report.trends.len(), 0);
    assert_eq!(report.insufficient_data.len(), 1);
    // Message format: "entity_id:metric (N samples)"
    assert!(report.insufficient_data[0].contains("samples"));
    Ok(())
}

#[tokio::test]
async fn test_max_observed_proxy_capacity_source() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: None, // No nominal capacity - should use max observed proxy
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    let values: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    insert_latency_observations(&storage, "test-device", "icmp", &values, base_time).await;

    let report = planner.analyze_capacity().await?;

    assert_eq!(report.trends.len(), 1);
    let trend = &report.trends[0];
    assert_eq!(trend.capacity_source, "max_observed_proxy");
    // Saturation threshold should be 80% of max observed (129ms * 0.8 = 103.2ms)
    assert!((trend.saturation_threshold - 103.2).abs() < 1.0);
    Ok(())
}

#[tokio::test]
async fn test_identical_timestamps_error() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    // All observations at the same timestamp (will cause regression error)
    ensure_device_exists(&storage, "test-device").await;
    for _ in 0..15 {
        let obs = netwatch::storage::Observation {
            id: Uuid::new_v4().to_string(),
            device_id: "test-device".to_string(),
            metric: "icmp_latency_ms".to_string(),
            value: 100.0,
            source: "test".to_string(),
            confidence: "high".to_string(),
            observed_at: base_time,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        storage.insert_observation(&obs).await.unwrap();
    }

    let report = planner.analyze_capacity().await?;

    // Should have insufficient_data_warning because all timestamps are identical
    assert_eq!(report.trends.len(), 0);
    assert_eq!(report.insufficient_data.len(), 1);
    // Message now contains regression error details
    let msg = &report.insufficient_data[0];
    eprintln!("Insufficient data message: {}", msg);
    assert!(
        msg.contains("regression") || msg.contains("test-device"),
        "Message: {}",
        msg
    );
    Ok(())
}

#[tokio::test]
async fn test_critical_resources_sorting() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);

    // Device 1: Fast growing (will saturate sooner)
    let values1: Vec<f64> = (0..30).map(|i| 500.0 + i as f64 * 10.0).collect();
    insert_latency_observations(&storage, "device-fast", "icmp", &values1, base_time).await;

    // Device 2: Slow growing (will saturate later)
    let values2: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 1.0).collect();
    insert_latency_observations(&storage, "device-slow", "icmp", &values2, base_time).await;

    let report = planner.analyze_capacity().await?;

    assert_eq!(report.critical_resources.len(), 2);
    // Fast growing should be first (sooner saturation)
    assert_eq!(report.critical_resources[0].entity_id, "device-fast");
    assert_eq!(report.critical_resources[1].entity_id, "device-slow");
    assert!(
        report.critical_resources[0].time_to_saturation.unwrap()
            < report.critical_resources[1].time_to_saturation.unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn test_report_markdown_generation() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    let values: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    insert_latency_observations(&storage, "test-device", "icmp", &values, base_time).await;

    let report = planner.analyze_capacity().await?;
    let markdown = report.to_markdown();

    assert!(markdown.contains("# Capacity Planning Report"));
    assert!(markdown.contains("test-device"));
    assert!(markdown.contains("icmp_latency_ms"));
    assert!(markdown.contains("nominal_capacity"));
    assert!(markdown.contains("80%"));
    assert!(markdown.contains("linear regression"));
    Ok(())
}

#[tokio::test]
async fn test_report_json_generation() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);
    let values: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    insert_latency_observations(&storage, "test-device", "icmp", &values, base_time).await;

    let report = planner.analyze_capacity().await?;
    let json = report.to_json()?;

    let parsed: serde_json::Value = serde_json::from_str(&json)?;
    assert!(parsed["trends"].is_array());
    assert_eq!(parsed["trends"].as_array().unwrap().len(), 1);
    assert_eq!(parsed["config_used"]["nominal_capacity"], 1000.0);
    Ok(())
}

#[tokio::test]
async fn test_multiple_protocols_separate_analysis() -> Result<()> {
    let dir = tempdir().unwrap();
    let storage = setup_test_storage(&dir).await;
    let config = CapacityConfig {
        min_samples: 10,
        lookback_window_seconds: 30 * 24 * 3600,
        saturation_threshold_pct: 80.0,
        confidence_level: 0.95,
        enabled: true,
        nominal_capacity: Some(1000.0),
    };
    let mut planner = CapacityPlanner::new(storage.clone(), config);

    let base_time = Utc::now() - Duration::days(30);

    // ICMP data
    let icmp_values: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    insert_latency_observations(&storage, "test-device", "icmp", &icmp_values, base_time).await;

    // HTTP data
    let http_values: Vec<f64> = (0..30).map(|i| 200.0 + i as f64 * 2.0).collect();
    insert_latency_observations(&storage, "test-device", "http", &http_values, base_time).await;

    let report = planner.analyze_capacity().await?;

    // Should have 2 separate trends (icmp and http)
    assert_eq!(report.trends.len(), 2);
    let protocols: Vec<String> = report.trends.iter().map(|t| t.protocol.clone()).collect();
    assert!(protocols.contains(&"icmp".to_string()));
    assert!(protocols.contains(&"http".to_string()));
    Ok(())
}
