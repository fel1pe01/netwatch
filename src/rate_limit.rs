use anyhow::Result;
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::warn;

/// Rate limiter configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum requests per window
    #[serde(default = "default_max_requests")]
    pub max_requests: u32,
    /// Time window in seconds
    #[serde(default = "default_window_seconds")]
    pub window_seconds: u64,
    /// Key to use for rate limiting: "ip", "user", "token" (default: "ip")
    #[serde(default = "default_key_type")]
    pub key_type: String,
    /// Paths to exclude from rate limiting
    #[serde(default)]
    pub excluded_paths: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_max_requests() -> u32 {
    100
}

fn default_window_seconds() -> u64 {
    60
}

fn default_key_type() -> String {
    "ip".to_string()
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_requests: 100,
            window_seconds: 60,
            key_type: "ip".to_string(),
            excluded_paths: vec!["/health".to_string(), "/metrics".to_string()],
        }
    }
}

/// Rate limiter state
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    // Store request counts per key
    buckets: Arc<DashMap<String, (u32, Instant)>>,
    // Cleanup interval
    cleanup_interval: Duration,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let buckets = Arc::new(DashMap::new());

        // Start background cleanup task
        let cleanup_buckets = buckets.clone();
        let cleanup_interval = Duration::from_secs(config.window_seconds * 2);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                let now = Instant::now();
                let window = Duration::from_secs(config.window_seconds);

                cleanup_buckets.retain(|_, (_, timestamp)| now.duration_since(*timestamp) < window);
            }
        });

        Self {
            config,
            buckets,
            cleanup_interval,
        }
    }

    /// Extract rate limit key from request
    fn extract_key(&self, headers: &HeaderMap, extensions: &axum::http::Extensions) -> String {
        match self.config.key_type.as_str() {
            "user" | "token" => {
                // Try to get authenticated user from extensions
                if let Some(user) = extensions.get::<crate::auth::AuthUser>() {
                    return format!("user:{}", user.username);
                }
                // Fall back to IP
            }
            _ => {} // "ip" or unknown - use IP
        }

        // Extract IP from headers
        let ip = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(|s| s.trim())
            .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
            .unwrap_or("unknown");

        format!("ip:{}", ip)
    }

    /// Check if request should be rate limited
    pub async fn check_rate_limit(
        &self,
        headers: &HeaderMap,
        extensions: &axum::http::Extensions,
        path: &str,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check excluded paths
        for excluded in &self.config.excluded_paths {
            if path.starts_with(excluded) {
                return Ok(());
            }
        }

        let key = self.extract_key(headers, extensions);
        let now = Instant::now();
        let window = Duration::from_secs(self.config.window_seconds);

        let mut entry = self.buckets.entry(key).or_insert((0, now));
        let (count, window_start) = entry.value_mut();

        // Reset count if window expired
        if now.duration_since(*window_start) >= window {
            *count = 0;
            *window_start = now;
        }

        *count += 1;

        if *count > self.config.max_requests {
            return Err(anyhow::anyhow!("Rate limit exceeded"));
        }

        Ok(())
    }

    /// Get current rate limit status for a key
    pub fn get_status(&self, key: &str) -> Option<(u32, u32)> {
        self.buckets.get(key).map(|entry| {
            let (count, _) = entry.value();
            (*count, self.config.max_requests)
        })
    }
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimiter>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();
    let extensions = request.extensions().clone();

    if let Err(_) = rate_limiter
        .check_rate_limit(&headers, &extensions, &path)
        .await
    {
        warn!("Rate limit exceeded for path: {}", path);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

/// Rate limiter state for use with Axum State
pub type RateLimitState = Arc<RateLimiter>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let config = RateLimitConfig {
            enabled: true,
            max_requests: 5,
            window_seconds: 60,
            key_type: "ip".to_string(),
            excluded_paths: vec![],
        };

        let limiter = RateLimiter::new(config);
        let headers = HeaderMap::new();
        let extensions = axum::http::Extensions::new();

        // First 5 requests should succeed
        for _ in 0..5 {
            assert!(limiter
                .check_rate_limit(&headers, &extensions, "/test")
                .await
                .is_ok());
        }

        // 6th request should fail
        assert!(limiter
            .check_rate_limit(&headers, &extensions, "/test")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_excluded_paths() {
        let config = RateLimitConfig {
            enabled: true,
            max_requests: 1,
            window_seconds: 60,
            key_type: "ip".to_string(),
            excluded_paths: vec!["/health".to_string()],
        };

        let limiter = RateLimiter::new(config);
        let headers = HeaderMap::new();
        let extensions = axum::http::Extensions::new();

        // Health path should be excluded
        assert!(limiter
            .check_rate_limit(&headers, &extensions, "/health")
            .await
            .is_ok());
        assert!(limiter
            .check_rate_limit(&headers, &extensions, "/health")
            .await
            .is_ok());

        // Other paths should be rate limited
        assert!(limiter
            .check_rate_limit(&headers, &extensions, "/test")
            .await
            .is_ok());
        assert!(limiter
            .check_rate_limit(&headers, &extensions, "/test")
            .await
            .is_err());
    }
}
