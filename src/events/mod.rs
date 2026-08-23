pub mod webhook;

#[cfg(feature = "amqp")]
pub mod amqp;

#[cfg(feature = "redis")]
pub mod redis;

use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tracing::warn;
use url::Url;
use uuid::Uuid;

use crate::audit::HistoryEvent;
use crate::config::Config;
use crate::storage::DynStorage;

pub type EventSinkResult = Result<(), String>;

/// Domain event emitted to external sinks after a successful mutation.
#[derive(Clone, Debug, Serialize)]
pub struct DomainEvent {
    pub id: Uuid,
    pub actor: String,
    pub resource_kind: String,
    pub resource_id: Option<Uuid>,
    pub resource_name: String,
    pub action: String,
    pub data: Value,
    pub timestamp: DateTime<Utc>,
}

impl From<&HistoryEvent> for DomainEvent {
    fn from(event: &HistoryEvent) -> Self {
        Self {
            id: event.id(),
            actor: event.actor().to_string(),
            resource_kind: event.resource_kind().to_string(),
            resource_id: event.resource_id(),
            resource_name: event.resource_name().to_string(),
            action: event.action().to_string(),
            data: event.data().clone(),
            timestamp: event.created_at(),
        }
    }
}

/// Async trait for emitting domain events to external systems.
///
/// Implementations report delivery failure so the transactional outbox can
/// retry it without coupling external availability to a mutation transaction.
#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit(&self, event: &DomainEvent) -> EventSinkResult;
}

/// Sink that discards all events. Used when no sinks are configured.
pub struct NoopSink;

#[async_trait]
impl EventSink for NoopSink {
    async fn emit(&self, _event: &DomainEvent) -> EventSinkResult {
        Ok(())
    }
}

/// Fans out events to multiple sinks concurrently.
pub struct CompositeSink {
    sinks: Vec<Arc<dyn EventSink>>,
}

impl CompositeSink {
    pub fn new(sinks: Vec<Arc<dyn EventSink>>) -> Self {
        Self { sinks }
    }
}

#[async_trait]
impl EventSink for CompositeSink {
    async fn emit(&self, event: &DomainEvent) -> EventSinkResult {
        let mut failures = Vec::new();
        for sink in &self.sinks {
            if let Err(error) = sink.emit(event).await {
                failures.push(error);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

/// Client wrapper for the active event sink, shared via `AppState`.
///
/// Follows the same `Arc<dyn Trait>` pattern as `AuthorizerClient`.
#[derive(Clone)]
pub struct EventSinkClient {
    inner: Arc<dyn EventSink>,
    outbox: Option<DynStorage>,
    notify: Arc<Notify>,
}

impl EventSinkClient {
    /// Create a client that discards all events.
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(NoopSink),
            outbox: None,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Wrap a caller-supplied sink. Intended for tests that need to inspect
    /// emitted events.
    pub fn with_sink(inner: Arc<dyn EventSink>) -> Self {
        Self {
            inner,
            outbox: None,
            notify: Arc::new(Notify::new()),
        }
    }

    /// Build an event sink client from configuration.
    ///
    /// Inspects `MREG_EVENT_*` env vars to determine which sinks to activate.
    /// If multiple are configured, wraps them in a `CompositeSink`.
    /// Falls back to `NoopSink` when nothing is configured.
    pub fn from_config(config: &Config, storage: DynStorage) -> Self {
        let mut sinks: Vec<Arc<dyn EventSink>> = Vec::new();

        if let Some(ref url) = config.event_webhook_url {
            sinks.push(Arc::new(webhook::WebhookSink::new(
                url.clone(),
                config.event_webhook_timeout_ms,
            )));
        }

        #[cfg(feature = "amqp")]
        if let Some(ref url) = config.event_amqp_url {
            sinks.push(Arc::new(amqp::AmqpSink::new(
                url.clone(),
                config.event_amqp_exchange.clone(),
            )));
        }

        #[cfg(feature = "redis")]
        if let Some(ref url) = config.event_redis_url {
            sinks.push(Arc::new(redis::RedisSink::new(
                url.clone(),
                config.event_redis_stream.clone(),
            )));
        }

        let inner: Arc<dyn EventSink> = match sinks.len() {
            0 => Arc::new(NoopSink),
            1 => sinks
                .into_iter()
                .next()
                .expect("len==1 guarantees at least one sink"),
            _ => Arc::new(CompositeSink::new(sinks)),
        };

        let client = Self {
            inner,
            outbox: Some(storage),
            notify: Arc::new(Notify::new()),
        };
        let worker = client.clone();
        tokio::spawn(async move { worker.run_outbox_worker().await });
        client
    }

    /// Wake the durable worker after the mutation commits. Test-only clients
    /// without storage deliver synchronously so assertions stay deterministic.
    pub async fn emit(&self, event: &DomainEvent) {
        if self.outbox.is_some() {
            self.notify.notify_one();
            return;
        }
        if let Err(error) = self.inner.emit(event).await {
            warn!(event_id = %event.id, %error, "direct event delivery failed");
        }
    }

    async fn run_outbox_worker(self) {
        let Some(storage) = self.outbox.clone() else {
            return;
        };
        loop {
            match storage.outbox().claim_events(16).await {
                Ok(claims) if claims.is_empty() => {
                    tokio::select! {
                        () = self.notify.notified() => {},
                        () = tokio::time::sleep(Duration::from_secs(5)) => {},
                    }
                }
                Ok(claims) => {
                    let deliveries = claims.into_iter().map(|claim| {
                        let sink = Arc::clone(&self.inner);
                        let storage = storage.clone();
                        async move {
                            let event = DomainEvent::from(claim.event());
                            match sink.emit(&event).await {
                                Ok(()) => {
                                    if let Err(error) = storage
                                        .outbox()
                                        .complete_event(event.id, claim.lease_id())
                                        .await
                                    {
                                        warn!(event_id = %event.id, %error, "failed to complete event delivery lease");
                                    }
                                }
                                Err(error) => {
                                    let exponent = claim.attempt().saturating_sub(1).min(8);
                                    let delay = 1_u32 << exponent;
                                    if let Err(store_error) = storage
                                        .outbox()
                                        .retry_event(event.id, claim.lease_id(), &error, delay)
                                        .await
                                    {
                                        warn!(event_id = %event.id, error = %store_error, "failed to reschedule event delivery");
                                    } else {
                                        warn!(event_id = %event.id, attempt = claim.attempt(), retry_seconds = delay, %error, "event delivery failed; retry scheduled");
                                    }
                                }
                            }
                        }
                    });
                    futures::future::join_all(deliveries).await;
                }
                Err(error) => {
                    warn!(%error, "failed to claim pending event deliveries");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}

pub(crate) fn safe_url_for_log(raw: &str) -> String {
    let Ok(url) = Url::parse(raw) else {
        return "<invalid-url>".to_string();
    };
    let host = url.host_str().unwrap_or("<unknown-host>");
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    format!("{}://{host}{port}/…", url.scheme())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Test sink that collects emitted events for assertions.
    struct CollectorSink {
        events: Arc<Mutex<Vec<DomainEvent>>>,
    }

    impl CollectorSink {
        fn new() -> (Self, Arc<Mutex<Vec<DomainEvent>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: events.clone(),
                },
                events,
            )
        }
    }

    #[async_trait]
    impl EventSink for CollectorSink {
        async fn emit(&self, event: &DomainEvent) -> EventSinkResult {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn noop_sink_does_not_panic() {
        let sink = NoopSink;
        let event = DomainEvent {
            id: Uuid::new_v4(),
            actor: "test".to_string(),
            resource_kind: "label".to_string(),
            resource_id: None,
            resource_name: "test-label".to_string(),
            action: "create".to_string(),
            data: serde_json::json!({}),
            timestamp: Utc::now(),
        };
        sink.emit(&event).await.unwrap();
    }

    #[tokio::test]
    async fn composite_sink_fans_out_to_all_sinks() {
        let (sink_a, events_a) = CollectorSink::new();
        let (sink_b, events_b) = CollectorSink::new();
        let composite = CompositeSink::new(vec![Arc::new(sink_a), Arc::new(sink_b)]);

        let event = DomainEvent {
            id: Uuid::new_v4(),
            actor: "test".to_string(),
            resource_kind: "host".to_string(),
            resource_id: Some(Uuid::new_v4()),
            resource_name: "web.example.org".to_string(),
            action: "create".to_string(),
            data: serde_json::json!({"name": "web.example.org"}),
            timestamp: Utc::now(),
        };
        composite.emit(&event).await.unwrap();

        assert_eq!(events_a.lock().unwrap().len(), 1);
        assert_eq!(events_b.lock().unwrap().len(), 1);
        assert_eq!(events_a.lock().unwrap()[0].resource_name, "web.example.org");
    }

    #[tokio::test]
    async fn event_sink_client_noop_does_not_panic() {
        let client = EventSinkClient::noop();
        let event = DomainEvent {
            id: Uuid::new_v4(),
            actor: "test".to_string(),
            resource_kind: "zone".to_string(),
            resource_id: None,
            resource_name: "example.org".to_string(),
            action: "delete".to_string(),
            data: serde_json::json!({}),
            timestamp: Utc::now(),
        };
        client.emit(&event).await;
    }

    #[test]
    fn domain_event_from_history_event() {
        let history = HistoryEvent::restore(
            Uuid::new_v4(),
            "admin".to_string(),
            "label".to_string(),
            Some(Uuid::new_v4()),
            "prod".to_string(),
            "create".to_string(),
            serde_json::json!({"name": "prod"}),
            Utc::now(),
        );
        let domain = DomainEvent::from(&history);
        assert_eq!(domain.id, history.id());
        assert_eq!(domain.actor, "admin");
        assert_eq!(domain.resource_kind, "label");
        assert_eq!(domain.action, "create");
    }

    #[test]
    fn log_url_redacts_credentials_path_and_query() {
        assert_eq!(
            safe_url_for_log("https://user:secret@hooks.example/private/token?key=value"),
            "https://hooks.example/…"
        );
    }
}
