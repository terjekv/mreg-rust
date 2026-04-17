use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskEnvelope {
    id: Uuid,
    kind: String,
    status: TaskStatus,
    payload: Value,
    progress: Value,
    result: Option<Value>,
    error_summary: Option<String>,
    attempts: i32,
    max_attempts: i32,
    available_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

impl TaskEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        kind: impl Into<String>,
        status: TaskStatus,
        payload: Value,
        progress: Value,
        result: Option<Value>,
        error_summary: Option<String>,
        attempts: i32,
        max_attempts: i32,
        available_at: DateTime<Utc>,
        started_at: Option<DateTime<Utc>>,
        finished_at: Option<DateTime<Utc>>,
    ) -> Result<Self, AppError> {
        let kind = kind.into().trim().to_string();
        if kind.is_empty() {
            return Err(AppError::validation("task kind cannot be empty"));
        }

        Ok(Self {
            id,
            kind,
            status,
            payload,
            progress,
            result,
            error_summary,
            attempts,
            max_attempts,
            available_at,
            started_at,
            finished_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn status(&self) -> &TaskStatus {
        &self.status
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub fn progress(&self) -> &Value {
        &self.progress
    }

    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub fn error_summary(&self) -> Option<&str> {
        self.error_summary.as_deref()
    }

    pub fn attempts(&self) -> i32 {
        self.attempts
    }

    pub fn max_attempts(&self) -> i32 {
        self.max_attempts
    }

    pub fn available_at(&self) -> DateTime<Utc> {
        self.available_at
    }

    pub fn started_at(&self) -> Option<DateTime<Utc>> {
        self.started_at
    }

    pub fn finished_at(&self) -> Option<DateTime<Utc>> {
        self.finished_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateTask {
    kind: String,
    requested_by: Option<String>,
    payload: Value,
    idempotency_key: Option<String>,
    max_attempts: i32,
}

impl CreateTask {
    pub fn new(
        kind: impl Into<String>,
        requested_by: Option<String>,
        payload: Value,
        idempotency_key: Option<String>,
        max_attempts: i32,
    ) -> Result<Self, AppError> {
        let kind = kind.into().trim().to_string();
        if kind.is_empty() {
            return Err(AppError::validation("task kind cannot be empty"));
        }
        if max_attempts <= 0 {
            return Err(AppError::validation("task max_attempts must be positive"));
        }

        Ok(Self {
            kind,
            requested_by: requested_by.map(|value| value.trim().to_string()),
            payload,
            idempotency_key: idempotency_key.map(|value| value.trim().to_string()),
            max_attempts,
        })
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn requested_by(&self) -> Option<&str> {
        self.requested_by.as_deref()
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref()
    }

    pub fn max_attempts(&self) -> i32 {
        self.max_attempts
    }
}
