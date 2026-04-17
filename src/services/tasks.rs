use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::{
        pagination::{Page, PageRequest},
        tasks::{CreateTask, TaskEnvelope},
    },
    errors::AppError,
    storage::TaskStore,
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "task"))]
pub async fn list(
    store: &(dyn TaskStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<TaskEnvelope>, AppError> {
    store.list_tasks(page).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "task"))]
pub async fn create(
    store: &(dyn TaskStore + Send + Sync),
    command: CreateTask,
) -> Result<TaskEnvelope, AppError> {
    store.create_task(command).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "task"))]
pub async fn claim_next(
    store: &(dyn TaskStore + Send + Sync),
) -> Result<Option<TaskEnvelope>, AppError> {
    store.claim_next_task().await
}

#[tracing::instrument(skip(store, result), fields(resource_kind = "task"))]
pub async fn complete(
    store: &(dyn TaskStore + Send + Sync),
    task_id: Uuid,
    result: Value,
) -> Result<TaskEnvelope, AppError> {
    store.complete_task(task_id, result).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "task"))]
pub async fn fail(
    store: &(dyn TaskStore + Send + Sync),
    task_id: Uuid,
    error_summary: String,
) -> Result<TaskEnvelope, AppError> {
    store.fail_task(task_id, error_summary).await
}
