use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    domain::{
        pagination::{Page, PageRequest},
        exports::ExportRun,
        imports::ImportBatchSummary,
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

fn cancel_task_in_state(state: &mut MemoryState, task_id: Uuid) -> Result<TaskEnvelope, AppError> {
    let current = state
        .tasks
        .get(&task_id)
        .cloned()
        .ok_or_else(|| AppError::not_found(format!("task '{}' was not found", task_id)))?;
    if current.status().is_terminal() {
        return Err(AppError::conflict(format!(
            "task '{}' is already finished and cannot be cancelled",
            task_id
        )));
    }

    let now = Utc::now();
    let updated = TaskEnvelope::restore(
        current.id(),
        current.kind().to_string(),
        TaskStatus::Cancelled,
        current.payload().clone(),
        current.progress().clone(),
        current.result().cloned(),
        current.error_summary().map(str::to_string),
        current.attempts(),
        current.max_attempts(),
        current.available_at(),
        current.started_at(),
        Some(now),
    )?;
    state.tasks.insert(task_id, updated.clone());
    Ok(updated)
}

fn clear_import_task_reference(
    summary: &ImportBatchSummary,
) -> Result<ImportBatchSummary, AppError> {
    Ok(ImportBatchSummary::restore(
        summary.id(),
        None,
        summary.status().clone(),
        summary.requested_by().map(str::to_string),
        summary.validation_report().cloned(),
        summary.commit_summary().cloned(),
        summary.created_at(),
        summary.updated_at(),
    ))
}

fn clear_export_task_reference(run: &ExportRun) -> Result<ExportRun, AppError> {
    ExportRun::restore(
        run.id(),
        None,
        run.template_id(),
        run.requested_by().map(str::to_string),
        run.scope().to_string(),
        run.parameters().clone(),
        run.status().clone(),
        run.rendered_output().map(str::to_string),
        run.artifact_metadata().cloned(),
        run.created_at(),
        run.updated_at(),
    )
}

fn purge_finished_tasks_before_in_state(
    state: &mut MemoryState,
    cutoff: DateTime<Utc>,
) -> Result<usize, AppError> {
    let purged_ids: Vec<Uuid> = state
        .tasks
        .values()
        .filter(|task| task.status().is_terminal())
        .filter_map(|task| match task.finished_at() {
            Some(finished_at) if finished_at < cutoff => Some(task.id()),
            _ => None,
        })
        .collect();

    if purged_ids.is_empty() {
        return Ok(0);
    }

    for task_id in &purged_ids {
        state.tasks.remove(task_id);
    }

    for stored in state.imports.values_mut() {
        if stored
            .summary
            .task_id()
            .is_some_and(|task_id| purged_ids.contains(&task_id))
        {
            let summary = stored.summary.clone();
            stored.summary = clear_import_task_reference(&summary)?;
        }
    }

    for run in state.export_runs.values_mut() {
        if run
            .task_id()
            .is_some_and(|task_id| purged_ids.contains(&task_id))
        {
            let current = run.clone();
            *run = clear_export_task_reference(&current)?;
        }
    }

    Ok(purged_ids.len())
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

    async fn cancel_task(&self, task_id: Uuid) -> Result<TaskEnvelope, AppError> {
        let mut state = self.state.write().await;
        cancel_task_in_state(&mut state, task_id)
    }

    async fn purge_finished_tasks_before(&self, cutoff: DateTime<Utc>) -> Result<usize, AppError> {
        let mut state = self.state.write().await;
        purge_finished_tasks_before_in_state(&mut state, cutoff)
    }
}
