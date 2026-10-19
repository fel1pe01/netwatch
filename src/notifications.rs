use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Telegram notification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
    #[serde(default = "default_parse_mode")]
    pub parse_mode: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_seconds: u64,
}

fn default_parse_mode() -> String {
    "HTML".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_backoff() -> u64 {
    5
}

/// Telegram message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMessage {
    pub chat_id: String,
    pub text: String,
    pub parse_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_web_page_preview: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_notification: Option<bool>,
}

/// Telegram API response
#[derive(Debug, Deserialize)]
struct TelegramResponse {
    ok: bool,
    #[serde(default)]
    description: String,
    #[serde(default)]
    error_code: i32,
}

/// Telegram notifier
pub struct TelegramNotifier {
    config: TelegramConfig,
    client: Client,
    /// Rate limiter: max messages per second
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

/// Simple token bucket rate limiter
struct RateLimiter {
    tokens: f64,
    last_refill: DateTime<Utc>,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
}

impl RateLimiter {
    fn new(max_per_second: f64) -> Self {
        Self {
            tokens: max_per_second,
            last_refill: Utc::now(),
            max_tokens: max_per_second,
            refill_rate: max_per_second,
        }
    }

    async fn acquire(&mut self) {
        loop {
            let now = Utc::now();
            let elapsed = (now - self.last_refill).num_milliseconds() as f64 / 1000.0;
            self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
            self.last_refill = now;

            if self.tokens >= 1.0 {
                self.tokens -= 1.0;
                return;
            }

            // Wait for token to be available
            let wait_ms = ((1.0 - self.tokens) / self.refill_rate * 1000.0) as u64;
            sleep(StdDuration::from_millis(wait_ms.max(10))).await;
        }
    }
}

impl TelegramNotifier {
    /// Create a new Telegram notifier
    pub fn new(config: TelegramConfig) -> Self {
        let client = Client::builder()
            .timeout(StdDuration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        // Telegram allows ~30 messages/second, but we'll be conservative
        let rate_limiter = Arc::new(Mutex::new(RateLimiter::new(20.0)));

        Self {
            config,
            client,
            rate_limiter,
        }
    }

    /// Send a message with retry logic
    pub async fn send_message(&self, text: &str) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let message = TelegramMessage {
            chat_id: self.config.chat_id.clone(),
            text: text.to_string(),
            parse_mode: self.config.parse_mode.clone(),
            disable_web_page_preview: Some(true),
            disable_notification: None,
        };

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let mut last_error = None;
        for attempt in 0..=self.config.max_retries {
            // Rate limit
            self.rate_limiter.lock().await.acquire().await;

            match self.send_request(&url, &message).await {
                Ok(response) => {
                    if response.ok {
                        info!("Telegram message sent successfully");
                        return Ok(());
                    } else {
                        last_error = Some(anyhow::anyhow!(
                            "Telegram API error: {} (code: {})",
                            response.description,
                            response.error_code
                        ));
                        // Don't retry on client errors (4xx)
                        if response.error_code >= 400 && response.error_code < 500 {
                            error!(
                                "Telegram client error (not retrying): {}",
                                last_error.as_ref().unwrap()
                            );
                            return Err(last_error.unwrap());
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                }
            }

            if attempt < self.config.max_retries {
                let backoff = self.config.retry_backoff_seconds * (2_u64.pow(attempt));
                warn!(
                    "Telegram send failed (attempt {}/{}): {}. Retrying in {}s...",
                    attempt + 1,
                    self.config.max_retries + 1,
                    last_error.as_ref().unwrap(),
                    backoff
                );
                sleep(StdDuration::from_secs(backoff)).await;
            }
        }

        error!(
            "Failed to send Telegram message after {} attempts: {}",
            self.config.max_retries + 1,
            last_error.as_ref().unwrap()
        );
        Err(last_error.unwrap())
    }

    async fn send_request(&self, url: &str, message: &TelegramMessage) -> Result<TelegramResponse> {
        let response = self
            .client
            .post(url)
            .json(message)
            .send()
            .await?
            .json::<TelegramResponse>()
            .await?;
        Ok(response)
    }

    /// Format an incident notification
    pub fn format_incident_opened(&self, incident: &IncidentNotification) -> String {
        format!(
            "<b>🚨 INCIDENT OPENED</b>\n\
            <b>ID:</b> <code>{}</code>\n\
            <b>Title:</b> {}\n\
            <b>Severity:</b> {}\n\
            <b>Entity:</b> {} ({})\n\
            <b>Protocol:</b> {}\n\
            <b>Time:</b> {}\n\
            <b>Description:</b> {}",
            incident.id,
            incident.title,
            incident.severity,
            incident.entity_id,
            incident.entity_type,
            incident.protocol,
            incident.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            incident.description.as_deref().unwrap_or("N/A")
        )
    }

    /// Format an incident acknowledged notification
    pub fn format_incident_acknowledged(&self, incident: &IncidentNotification) -> String {
        format!(
            "<b>✋ INCIDENT ACKNOWLEDGED</b>\n\
            <b>ID:</b> <code>{}</code>\n\
            <b>Title:</b> {}\n\
            <b>Acknowledged by:</b> {}\n\
            <b>Reason:</b> {}\n\
            <b>Time:</b> {}",
            incident.id,
            incident.title,
            incident.acknowledged_by.as_deref().unwrap_or("unknown"),
            incident.acknowledged_reason.as_deref().unwrap_or("N/A"),
            incident.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }

    /// Format an incident resolved notification
    pub fn format_incident_resolved(&self, incident: &IncidentNotification) -> String {
        format!(
            "<b>✅ INCIDENT RESOLVED</b>\n\
            <b>ID:</b> <code>{}</code>\n\
            <b>Title:</b> {}\n\
            <b>Resolved by:</b> {}\n\
            <b>Reason:</b> {}\n\
            <b>Time:</b> {}",
            incident.id,
            incident.title,
            incident.resolved_by.as_deref().unwrap_or("auto"),
            incident
                .resolved_reason
                .as_deref()
                .unwrap_or("Condition recovered"),
            incident.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }

    /// Format an incident closed notification
    pub fn format_incident_closed(&self, incident: &IncidentNotification) -> String {
        format!(
            "<b>🔒 INCIDENT CLOSED</b>\n\
            <b>ID:</b> <code>{}</code>\n\
            <b>Title:</b> {}\n\
            <b>Closed by:</b> {}\n\
            <b>Reason:</b> {}\n\
            <b>Time:</b> {}",
            incident.id,
            incident.title,
            incident.closed_by.as_deref().unwrap_or("unknown"),
            incident.closed_reason.as_deref().unwrap_or("N/A"),
            incident.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
}

/// Incident notification data
#[derive(Debug, Clone)]
pub struct IncidentNotification {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub entity_id: String,
    pub entity_type: String,
    pub protocol: String,
    pub description: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub acknowledged_by: Option<String>,
    pub acknowledged_reason: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_reason: Option<String>,
    pub closed_by: Option<String>,
    pub closed_reason: Option<String>,
}

/// Notification manager - handles multiple notification channels
pub struct NotificationManager {
    telegram: Option<Arc<TelegramNotifier>>,
    // Future: slack, email, webhook, etc.
}

impl NotificationManager {
    pub fn new() -> Self {
        Self { telegram: None }
    }

    pub fn set_telegram(&mut self, notifier: TelegramNotifier) {
        self.telegram = Some(Arc::new(notifier));
    }

    pub async fn notify_incident_opened(&self, incident: &IncidentNotification) -> Result<()> {
        if let Some(telegram) = &self.telegram {
            let message = telegram.format_incident_opened(incident);
            if let Err(e) = telegram.send_message(&message).await {
                error!(
                    "Failed to send Telegram notification for incident opened: {}",
                    e
                );
            }
        }
        Ok(())
    }

    pub async fn notify_incident_acknowledged(
        &self,
        incident: &IncidentNotification,
    ) -> Result<()> {
        if let Some(telegram) = &self.telegram {
            let message = telegram.format_incident_acknowledged(incident);
            if let Err(e) = telegram.send_message(&message).await {
                error!(
                    "Failed to send Telegram notification for incident acknowledged: {}",
                    e
                );
            }
        }
        Ok(())
    }

    pub async fn notify_incident_resolved(&self, incident: &IncidentNotification) -> Result<()> {
        if let Some(telegram) = &self.telegram {
            let message = telegram.format_incident_resolved(incident);
            if let Err(e) = telegram.send_message(&message).await {
                error!(
                    "Failed to send Telegram notification for incident resolved: {}",
                    e
                );
            }
        }
        Ok(())
    }

    pub async fn notify_incident_closed(&self, incident: &IncidentNotification) -> Result<()> {
        if let Some(telegram) = &self.telegram {
            let message = telegram.format_incident_closed(incident);
            if let Err(e) = telegram.send_message(&message).await {
                error!(
                    "Failed to send Telegram notification for incident closed: {}",
                    e
                );
            }
        }
        Ok(())
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(10.0);
        // Should allow 10 immediate requests
        for _ in 0..10 {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(limiter.acquire());
        }
        // 11th should wait
        let start = std::time::Instant::now();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(limiter.acquire());
        let elapsed = start.elapsed();
        assert!(elapsed.as_millis() >= 50); // Should wait at least some time
    }

    #[test]
    fn test_telegram_message_serialization() {
        let msg = TelegramMessage {
            chat_id: "12345".to_string(),
            text: "Test message".to_string(),
            parse_mode: "HTML".to_string(),
            disable_web_page_preview: Some(true),
            disable_notification: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("12345"));
        assert!(json.contains("Test message"));
        assert!(json.contains("HTML"));
    }
}
