use std::time::Duration;

use async_trait::async_trait;
use tracing::warn;

use super::{DomainEvent, EventSink};

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
    async fn emit(&self, event: &DomainEvent) {
        if let Err(error) = self.try_post(event).await {
            warn!(
                url = %self.url,
                resource_kind = %event.resource_kind,
                action = %event.action,
                %error,
                "webhook delivery failed, retrying once"
            );
            tokio::time::sleep(Duration::from_secs(1)).await;
            if let Err(retry_error) = self.try_post(event).await {
                warn!(
                    url = %self.url,
                    resource_kind = %event.resource_kind,
                    action = %event.action,
                    error = %retry_error,
                    "webhook delivery failed on retry, dropping event"
                );
            }
        }
    }
}
