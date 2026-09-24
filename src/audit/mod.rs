use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Conventional actor identifiers used by service-layer mutations.
pub mod actor {
    use std::{cell::RefCell, future::Future};
    use tokio::task_local;

    pub const SYSTEM: &str = "system";

    task_local! {
        static REQUEST_ACTOR: String;
    }

    thread_local! {
        static TRANSACTION_ACTOR: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    struct TransactionActorReset(Option<String>);

    impl Drop for TransactionActorReset {
        fn drop(&mut self) {
            TRANSACTION_ACTOR.with(|slot| {
                slot.replace(self.0.take());
            });
        }
    }

    /// Run a request future with the authenticated principal available to
    /// service-layer audit recording.
    pub async fn scope<F: Future>(actor: String, future: F) -> F::Output {
        REQUEST_ACTOR.scope(actor, future).await
    }

    /// Return the active authenticated actor, falling back to `system` for
    /// background jobs and direct service calls outside an HTTP request.
    pub fn current() -> String {
        TRANSACTION_ACTOR
            .with(|actor| actor.borrow().clone())
            .or_else(|| REQUEST_ACTOR.try_with(Clone::clone).ok())
            .unwrap_or_else(|| SYSTEM.to_string())
    }

    pub(crate) fn with_transaction_actor<T>(actor: String, work: impl FnOnce() -> T) -> T {
        let previous = TRANSACTION_ACTOR.with(|slot| slot.replace(Some(actor)));
        let _reset = TransactionActorReset(previous);
        work()
    }
}

/// Conventional action verbs recorded on audit events.
pub mod actions {
    pub const CREATE: &str = "create";
    pub const UPDATE: &str = "update";
    pub const DELETE: &str = "delete";
    pub const ADD_ATOM: &str = "add_atom";
    pub const REMOVE_ATOM: &str = "remove_atom";
    pub const ADD_HOST: &str = "add_host";
    pub const REMOVE_HOST: &str = "remove_host";
    pub const ADD_LABEL: &str = "add_label";
    pub const REMOVE_LABEL: &str = "remove_label";
}

/// Immutable audit trail entry recording a mutation with actor, resource, and action details.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEvent {
    id: Uuid,
    actor: String,
    resource_kind: String,
    resource_id: Option<Uuid>,
    resource_name: String,
    action: String,
    data: Value,
    created_at: DateTime<Utc>,
}

/// A leased audit event awaiting delivery to configured external sinks.
#[derive(Clone, Debug)]
pub struct OutboxClaim {
    event: HistoryEvent,
    lease_id: Uuid,
    attempt: u32,
}

impl OutboxClaim {
    pub fn new(event: HistoryEvent, lease_id: Uuid, attempt: u32) -> Self {
        Self {
            event,
            lease_id,
            attempt,
        }
    }

    pub fn event(&self) -> &HistoryEvent {
        &self.event
    }

    pub fn lease_id(&self) -> Uuid {
        self.lease_id
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

impl HistoryEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        actor: String,
        resource_kind: String,
        resource_id: Option<Uuid>,
        resource_name: String,
        action: String,
        data: Value,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            actor,
            resource_kind,
            resource_id,
            resource_name,
            action,
            data,
            created_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn actor(&self) -> &str {
        &self.actor
    }
    pub fn resource_kind(&self) -> &str {
        &self.resource_kind
    }
    pub fn resource_id(&self) -> Option<Uuid> {
        self.resource_id
    }
    pub fn resource_name(&self) -> &str {
        &self.resource_name
    }
    pub fn action(&self) -> &str {
        &self.action
    }
    pub fn data(&self) -> &Value {
        &self.data
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// Command to record a new audit event.
#[derive(Clone, Debug)]
pub struct CreateHistoryEvent {
    actor: String,
    resource_kind: String,
    resource_id: Option<Uuid>,
    resource_name: String,
    action: String,
    data: Value,
}

impl CreateHistoryEvent {
    pub fn new(
        actor: impl Into<String>,
        resource_kind: impl Into<String>,
        resource_id: Option<Uuid>,
        resource_name: impl Into<String>,
        action: impl Into<String>,
        data: Value,
    ) -> Self {
        Self {
            actor: actor.into(),
            resource_kind: resource_kind.into(),
            resource_id,
            resource_name: resource_name.into(),
            action: action.into(),
            data,
        }
    }

    pub fn actor(&self) -> &str {
        &self.actor
    }
    pub fn resource_kind(&self) -> &str {
        &self.resource_kind
    }
    pub fn resource_id(&self) -> Option<Uuid> {
        self.resource_id
    }
    pub fn resource_name(&self) -> &str {
        &self.resource_name
    }
    pub fn action(&self) -> &str {
        &self.action
    }
    pub fn data(&self) -> &Value {
        &self.data
    }
}
