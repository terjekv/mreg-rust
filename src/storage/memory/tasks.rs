use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    domain::{
        pagination::{Page, PageRequest},
        tasks::{CreateTask, TaskEnvelope, TaskStatus},
    },
    errors::AppError,
    storage::TaskStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor};

pub(super) fn create_task_in_state(
    state: &mut MemoryState,
    command: CreateTask,
) -> Result<TaskEnvelope, AppError> {
    if let Some(idempotency_key) = command.idempotency_key()
        && state
            .tasks
            .values()
            .any(|task| task.payload()["idempotency_key"] == idempotency_key)
    {
        return Err(AppError::conflict(format!(
            "task idempotency key '{}' already exists",
            idempotency_key
        )));
    }
    let now = Utc::now();
    let payload = if let Some(key) = command.idempotency_key() {
        let mut payload = command.payload().clone();
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "idempotency_key".to_string(),
                Value::String(key.to_string()),
            );
        }
        payload
    } else {
        command.payload().clone()
    };
    let task = TaskEnvelope::restore(
        Uuid::new_v4(),
        command.kind().to_string(),
        TaskStatus::Queued,
        payload,
        json!({"stage":"queued"}),
        None,
        None,
        0,
        command.max_attempts(),
        now,
        None,
        None,
    )?;
    state.tasks.insert(task.id(), task.clone());
    Ok(task)
}

#[async_trait]
impl TaskStore for MemoryStorage {
    async fn list_tasks(&self, page: &PageRequest) -> Result<Page<TaskEnvelope>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<TaskEnvelope> = state.tasks.values().cloned().collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn create_task(&self, command: CreateTask) -> Result<TaskEnvelope, AppError> {
        let mut state = self.state.write().await;
        create_task_in_state(&mut state, command)
    }

    async fn claim_next_task(&self) -> Result<Option<TaskEnvelope>, AppError> {
        let mut state = self.state.write().await;
        let now = Utc::now();
        let next_task_id = state
            .tasks
            .values()
            .filter(|task| {
                matches!(task.status(), TaskStatus::Queued) && task.available_at() <= now
            })
            .min_by_key(|task| (task.available_at(), task.id()))
            .map(|task| task.id());

        let Some(task_id) = next_task_id else {
            return Ok(None);
        };

        let current =
            state.tasks.get(&task_id).cloned().ok_or_else(|| {
                AppError::internal("claimed task disappeared from in-memory storage")
            })?;
        let updated = TaskEnvelope::restore(
            current.id(),
            current.kind().to_string(),
            TaskStatus::Running,
            current.payload().clone(),
            current.progress().clone(),
            current.result().cloned(),
            current.error_summary().map(str::to_string),
            current.attempts() + 1,
            current.max_attempts(),
            current.available_at(),
            Some(now),
            None,
        )?;
        state.tasks.insert(task_id, updated.clone());
        Ok(Some(updated))
    }

    async fn complete_task(&self, task_id: Uuid, result: Value) -> Result<TaskEnvelope, AppError> {
        let mut state = self.state.write().await;
        let current = state
            .tasks
            .get(&task_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("task '{}' was not found", task_id)))?;
        let now = Utc::now();
        let updated = TaskEnvelope::restore(
            current.id(),
            current.kind().to_string(),
            TaskStatus::Succeeded,
            current.payload().clone(),
            current.progress().clone(),
            Some(result),
            None,
            current.attempts(),
            current.max_attempts(),
            current.available_at(),
            current.started_at(),
            Some(now),
        )?;
        state.tasks.insert(task_id, updated.clone());
        Ok(updated)
    }

    async fn fail_task(
        &self,
        task_id: Uuid,
        error_summary: String,
    ) -> Result<TaskEnvelope, AppError> {
        let mut state = self.state.write().await;
        let current = state
            .tasks
            .get(&task_id)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("task '{}' was not found", task_id)))?;
        let now = Utc::now();
        let updated = TaskEnvelope::restore(
            current.id(),
            current.kind().to_string(),
            TaskStatus::Failed,
            current.payload().clone(),
            current.progress().clone(),
            None,
            Some(error_summary),
            current.attempts(),
            current.max_attempts(),
            current.available_at(),
            current.started_at(),
            Some(now),
        )?;
        state.tasks.insert(task_id, updated.clone());
        Ok(updated)
    }
}
