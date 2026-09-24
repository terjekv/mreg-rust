use std::time::Duration;

use super::{DomainEvent, EventSink, safe_url_for_log};
use async_trait::async_trait;

/// Emits events by POSTing JSON to a webhook URL.
pub struct WebhookSink {
    client: reqwest::Client,
    url: String,
}

impl WebhookSink {
    pub fn new(url: String, timeout_ms: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, url }
    }

    async fn try_post(&self, event: &DomainEvent) -> Result<(), reqwest::Error> {
        self.client
            .post(&self.url)
            .json(event)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

#[async_trait]
impl EventSink for WebhookSink {
    async fn emit(&self, event: &DomainEvent) -> Result<(), String> {
        self.try_post(event)
            .await
            .map_err(|error| format!("webhook {}: {error}", safe_url_for_log(&self.url)))
    }
}
