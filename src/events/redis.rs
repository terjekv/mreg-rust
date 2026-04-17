use async_trait::async_trait;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tracing::warn;

use super::{DomainEvent, EventSink};

/// Emits events to a Redis Stream via `XADD`.
pub struct RedisSink {
    url: String,
    stream: String,
    connection: Mutex<Option<redis::aio::MultiplexedConnection>>,
}

impl RedisSink {
    pub fn new(url: String, stream: String) -> Self {
        Self {
            url,
            stream,
            connection: Mutex::new(None),
        }
    }

    async fn get_or_connect(&self) -> Result<redis::aio::MultiplexedConnection, redis::RedisError> {
        let mut guard = self.connection.lock().await;
        if let Some(ref conn) = *guard {
            return Ok(conn.clone());
        }

        let client = redis::Client::open(self.url.as_str())?;
        let conn = client.get_multiplexed_async_connection().await?;
        *guard = Some(conn.clone());
        Ok(conn)
    }
}

#[async_trait]
impl EventSink for RedisSink {
    async fn emit(&self, event: &DomainEvent) {
        let payload = match serde_json::to_string(event) {
            Ok(json) => json,
            Err(error) => {
                warn!(%error, "failed to serialize event for Redis");
                return;
            }
        };

        let mut conn = match self.get_or_connect().await {
            Ok(c) => c,
            Err(error) => {
                warn!(url = %self.url, %error, "Redis connection failed, dropping event");
                return;
            }
        };

        let result: Result<String, redis::RedisError> = conn
            .xadd(
                &self.stream,
                "*", // auto-generate stream ID
                &[
                    ("id", event.id.to_string().as_str()),
                    ("resource_kind", &event.resource_kind),
                    ("resource_name", &event.resource_name),
                    ("action", &event.action),
                    ("actor", &event.actor),
                    ("payload", &payload),
                ],
            )
            .await;

        if let Err(error) = result {
            warn!(
                stream = %self.stream,
                %error,
                "Redis XADD failed, dropping event"
            );
            // Clear cached connection so next emit reconnects
            *self.connection.lock().await = None;
        }
    }
}
