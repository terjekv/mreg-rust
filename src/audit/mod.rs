use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

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
