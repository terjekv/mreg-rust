use async_trait::async_trait;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    options::{BasicPublishOptions, ExchangeDeclareOptions},
    types::FieldTable,
};
use tokio::sync::Mutex;
use tracing::warn;

use super::{DomainEvent, EventSink};

/// Emits events to an AMQP exchange with routing key `{resource_kind}.{action}`.
pub struct AmqpSink {
    url: String,
    exchange: String,
    channel: Mutex<Option<Channel>>,
}

impl AmqpSink {
    pub fn new(url: String, exchange: String) -> Self {
        Self {
            url,
            exchange,
            channel: Mutex::new(None),
        }
    }

    async fn get_or_connect(&self) -> Result<Channel, lapin::Error> {
        let mut guard = self.channel.lock().await;
        if let Some(ref channel) = *guard
            && channel.status().connected()
        {
            return Ok(channel.clone());
        }

        let connection = Connection::connect(&self.url, ConnectionProperties::default()).await?;
        let channel = connection.create_channel().await?;
        channel
            .exchange_declare(
                &self.exchange,
                ExchangeKind::Topic,
                ExchangeDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await?;
        *guard = Some(channel.clone());
        Ok(channel)
    }
}

#[async_trait]
impl EventSink for AmqpSink {
    async fn emit(&self, event: &DomainEvent) {
        let routing_key = format!("{}.{}", event.resource_kind, event.action);
        let payload = match serde_json::to_vec(event) {
            Ok(bytes) => bytes,
            Err(error) => {
                warn!(%error, "failed to serialize event for AMQP");
                return;
            }
        };

        let channel = match self.get_or_connect().await {
            Ok(ch) => ch,
            Err(error) => {
                warn!(url = %self.url, %error, "AMQP connection failed, dropping event");
                return;
            }
        };

        if let Err(error) = channel
            .basic_publish(
                &self.exchange,
                &routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_delivery_mode(2), // persistent
            )
            .await
        {
            warn!(
                exchange = %self.exchange,
                %routing_key,
                %error,
                "AMQP publish failed, dropping event"
            );
            // Clear cached channel so next emit reconnects
            *self.channel.lock().await = None;
        }
    }
}
