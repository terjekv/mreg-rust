use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    domain::{
        pagination::{Page, PageRequest},
        tasks::{CreateTask, TaskEnvelope},
    },
    errors::AppError,
};

/// Asynchronous task queue operations (create, claim, complete, fail, cancel, purge).
#[async_trait]
pub trait TaskStore: Send + Sync {
    async fn list_tasks(&self, page: &PageRequest) -> Result<Page<TaskEnvelope>, AppError>;
    async fn create_task(&self, command: CreateTask) -> Result<TaskEnvelope, AppError>;
    async fn claim_next_task(&self) -> Result<Option<TaskEnvelope>, AppError>;
    async fn complete_task(
        &self,
        task_id: uuid::Uuid,
        result: serde_json::Value,
    ) -> Result<TaskEnvelope, AppError>;
    async fn fail_task(
        &self,
        task_id: uuid::Uuid,
        error_summary: String,
    ) -> Result<TaskEnvelope, AppError>;
    async fn cancel_task(&self, task_id: uuid::Uuid) -> Result<TaskEnvelope, AppError>;
    async fn purge_finished_tasks_before(&self, cutoff: DateTime<Utc>) -> Result<usize, AppError>;
}
