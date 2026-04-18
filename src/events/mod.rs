pub mod webhook;

#[cfg(feature = "amqp")]
pub mod amqp;

#[cfg(feature = "redis")]
pub mod redis;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::audit::HistoryEvent;
use crate::config::Config;

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
/// Implementations must be fire-and-forget: errors are logged internally
/// and never propagated to callers. A sink failure must not block a mutation.
#[async_trait]
pub trait EventSink: Send + Sync {
    async fn emit(&self, event: &DomainEvent);
}

/// Sink that discards all events. Used when no sinks are configured.
pub struct NoopSink;

#[async_trait]
impl EventSink for NoopSink {
    async fn emit(&self, _event: &DomainEvent) {}
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
    async fn emit(&self, event: &DomainEvent) {
        let mut tasks = Vec::with_capacity(self.sinks.len());
        for sink in &self.sinks {
            let sink = Arc::clone(sink);
            let event = event.clone();
            tasks.push(tokio::spawn(async move {
                sink.emit(&event).await;
            }));
        }

        for task in tasks {
            let _ = task.await;
        }
    }
}

/// Client wrapper for the active event sink, shared via `AppState`.
///
/// Follows the same `Arc<dyn Trait>` pattern as `AuthorizerClient`.
#[derive(Clone)]
pub struct EventSinkClient {
    inner: Arc<dyn EventSink>,
}

impl EventSinkClient {
    /// Create a client that discards all events.
    pub fn noop() -> Self {
        Self {
            inner: Arc::new(NoopSink),
        }
    }

    /// Wrap a caller-supplied sink. Intended for tests that need to inspect
    /// emitted events.
    pub fn with_sink(inner: Arc<dyn EventSink>) -> Self {
        Self { inner }
    }

    /// Build an event sink client from configuration.
    ///
    /// Inspects `MREG_EVENT_*` env vars to determine which sinks to activate.
    /// If multiple are configured, wraps them in a `CompositeSink`.
    /// Falls back to `NoopSink` when nothing is configured.
    pub fn from_config(config: &Config) -> Self {
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

        Self { inner }
    }

    /// Schedule background delivery of a domain event. Never fails callers.
    pub async fn emit(&self, event: &DomainEvent) {
        let sink = Arc::clone(&self.inner);
        let event = event.clone();
        tokio::spawn(async move {
            sink.emit(&event).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    use tokio::sync::Notify;

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
        async fn emit(&self, event: &DomainEvent) {
            self.events.lock().unwrap().push(event.clone());
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
        sink.emit(&event).await;
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
        composite.emit(&event).await;

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

    struct SlowSink {
        delivered: Arc<AtomicBool>,
        notify: Arc<Notify>,
    }

    #[async_trait]
    impl EventSink for SlowSink {
        async fn emit(&self, _event: &DomainEvent) {
            tokio::time::sleep(Duration::from_millis(50)).await;
            self.delivered.store(true, Ordering::SeqCst);
            self.notify.notify_one();
        }
    }

    #[tokio::test]
    async fn event_sink_client_returns_before_delivery_finishes() {
        let delivered = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(Notify::new());
        let client = EventSinkClient {
            inner: Arc::new(SlowSink {
                delivered: Arc::clone(&delivered),
                notify: Arc::clone(&notify),
            }),
        };
        let event = DomainEvent {
            id: Uuid::new_v4(),
            actor: "test".to_string(),
            resource_kind: "zone".to_string(),
            resource_id: None,
            resource_name: "example.org".to_string(),
            action: "update".to_string(),
            data: serde_json::json!({}),
            timestamp: Utc::now(),
        };

        let started = Instant::now();
        client.emit(&event).await;

        assert!(started.elapsed() < Duration::from_millis(20));
        assert!(!delivered.load(Ordering::SeqCst));
        tokio::time::timeout(Duration::from_millis(250), notify.notified())
            .await
            .expect("background delivery should complete");
        assert!(delivered.load(Ordering::SeqCst));
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
}
