use super::{DomainEvent, EventSink, safe_url_for_log};
use async_trait::async_trait;
use lapin::{
    BasicProperties, Channel, Connection, ConnectionProperties, ExchangeKind,
    options::{BasicPublishOptions, ConfirmSelectOptions, ExchangeDeclareOptions},
    publisher_confirm::Confirmation,
    types::FieldTable,
};
use tokio::sync::Mutex;

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
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await?;
        *guard = Some(channel.clone());
        Ok(channel)
    }
}

#[async_trait]
impl EventSink for AmqpSink {
    async fn emit(&self, event: &DomainEvent) -> Result<(), String> {
        let routing_key = format!("{}.{}", event.resource_kind, event.action);
        let payload = serde_json::to_vec(event)
            .map_err(|error| format!("failed to serialize event for AMQP: {error}"))?;

        let channel = self
            .get_or_connect()
            .await
            .map_err(|error| format!("AMQP connection {}: {error}", safe_url_for_log(&self.url)))?;

        let publish = channel
            .basic_publish(
                &self.exchange,
                &routing_key,
                BasicPublishOptions::default(),
                &payload,
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_delivery_mode(2), // persistent
            )
            .await;
        let confirm = match publish {
            Ok(confirm) => confirm,
            Err(error) => {
                *self.channel.lock().await = None;
                return Err(format!("AMQP publish to {}: {error}", self.exchange));
            }
        };
        match confirm.await {
            Ok(Confirmation::Ack(None)) => Ok(()),
            Ok(Confirmation::Ack(Some(_))) => Err(format!(
                "AMQP publish to {} was returned as unroutable",
                self.exchange
            )),
            Ok(Confirmation::Nack(_)) => Err(format!(
                "AMQP broker negatively acknowledged publish to {}",
                self.exchange
            )),
            Ok(Confirmation::NotRequested) => {
                Err("AMQP publisher confirms are disabled".to_string())
            }
            Err(error) => {
                // Clear cached channel so the retry reconnects.
                let message = format!("AMQP confirmation for {}: {error}", self.exchange);
                *self.channel.lock().await = None;
                Err(message)
            }
        }
    }
}
