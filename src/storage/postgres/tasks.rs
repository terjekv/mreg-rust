use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    delete, insert_into, sql_query, update,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{models::TaskRow, schema::tasks},
    domain::{
        pagination::{Page, PageRequest},
        tasks::{CreateTask, TaskEnvelope},
    },
    errors::AppError,
    storage::TaskStore,
};

use super::PostgresStorage;
use super::helpers::vec_to_page;

impl PostgresStorage {
    pub(super) fn query_tasks(
        connection: &mut PgConnection,
    ) -> Result<Vec<TaskEnvelope>, AppError> {
        let rows = tasks::table
            .select(TaskRow::as_select())
            .order((tasks::available_at.asc(), tasks::created_at.asc()))
            .load::<TaskRow>(connection)?;
        rows.into_iter().map(TaskRow::into_domain).collect()
    }

    pub(super) fn create_task_row(
        connection: &mut PgConnection,
        command: &CreateTask,
        payload_override: Option<Value>,
    ) -> Result<TaskEnvelope, AppError> {
        let payload = payload_override.unwrap_or_else(|| command.payload().clone());
        insert_into(tasks::table)
            .values((
                tasks::kind.eq(command.kind()),
                tasks::status.eq("queued"),
                tasks::idempotency_key.eq(command.idempotency_key()),
                tasks::requested_by.eq(command.requested_by()),
                tasks::payload.eq(&payload),
                tasks::progress.eq(json!({"stage":"queued"})),
                tasks::max_attempts.eq(command.max_attempts()),
            ))
            .returning(TaskRow::as_returning())
            .get_result(connection)
            .map_err(|error| match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::UniqueViolation,
                    _,
                ) => AppError::conflict("task already exists for the supplied idempotency key"),
                other => AppError::internal(other),
            })?
            .into_domain()
    }

    pub(super) fn get_task_by_id(
        connection: &mut PgConnection,
        task_id: Uuid,
    ) -> Result<Option<TaskEnvelope>, AppError> {
        let row = tasks::table
            .filter(tasks::id.eq(task_id))
            .select(TaskRow::as_select())
            .first::<TaskRow>(connection)
            .optional()?;
        row.map(TaskRow::into_domain).transpose()
    }
}

#[async_trait]
impl TaskStore for PostgresStorage {
    async fn list_tasks(&self, page: &PageRequest) -> Result<Page<TaskEnvelope>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_tasks(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn create_task(&self, command: CreateTask) -> Result<TaskEnvelope, AppError> {
        self.database
            .run(move |connection| Self::create_task_row(connection, &command, None))
            .await
    }

    // NOTE: claim_next_task MUST stay as sql_query() because Diesel DSL
    // does not support FOR UPDATE SKIP LOCKED row-level locking syntax.
    async fn claim_next_task(&self) -> Result<Option<TaskEnvelope>, AppError> {
        self.database
            .run(|connection| {
                let row = sql_query(
                    "WITH next_task AS (
                        SELECT id
                        FROM tasks
                        WHERE status = 'queued'
                          AND available_at <= now()
                        ORDER BY available_at, created_at
                        FOR UPDATE SKIP LOCKED
                        LIMIT 1
                     )
                     UPDATE tasks t
                     SET status = 'running',
                         attempts = t.attempts + 1,
                         started_at = COALESCE(t.started_at, now()),
                         updated_at = now()
                     FROM next_task
                     WHERE t.id = next_task.id
                     RETURNING t.id, t.kind, t.status, t.payload, t.progress, t.result,
                               t.error_summary, t.attempts, t.max_attempts, t.available_at,
                               t.started_at, t.finished_at",
                )
                .get_result::<TaskRow>(connection)
                .optional()?;
                row.map(TaskRow::into_domain).transpose()
            })
            .await
    }

    async fn complete_task(
        &self,
        task_id: Uuid,
        result: serde_json::Value,
    ) -> Result<TaskEnvelope, AppError> {
        self.database
            .run(move |connection| {
                update(tasks::table.filter(tasks::id.eq(task_id)))
                    .set((
                        tasks::status.eq("succeeded"),
                        tasks::result.eq(Some(&result)),
                        tasks::error_summary.eq(None::<String>),
                        tasks::finished_at.eq(diesel::dsl::now),
                        tasks::updated_at.eq(diesel::dsl::now),
                    ))
                    .returning(TaskRow::as_returning())
                    .get_result::<TaskRow>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("task '{}' was not found", task_id))
                    })?
                    .into_domain()
            })
            .await
    }

    async fn fail_task(
        &self,
        task_id: Uuid,
        error_summary: String,
    ) -> Result<TaskEnvelope, AppError> {
        self.database
            .run(move |connection| {
                update(tasks::table.filter(tasks::id.eq(task_id)))
                    .set((
                        tasks::status.eq("failed"),
                        tasks::error_summary.eq(Some(&error_summary)),
                        tasks::finished_at.eq(diesel::dsl::now),
                        tasks::updated_at.eq(diesel::dsl::now),
                    ))
                    .returning(TaskRow::as_returning())
                    .get_result::<TaskRow>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("task '{}' was not found", task_id))
                    })?
                    .into_domain()
            })
            .await
    }

    async fn cancel_task(&self, task_id: Uuid) -> Result<TaskEnvelope, AppError> {
        self.database
            .run(move |connection| {
                let current = Self::get_task_by_id(connection, task_id)?.ok_or_else(|| {
                    AppError::not_found(format!("task '{}' was not found", task_id))
                })?;
                if current.status().is_terminal() {
                    return Err(AppError::conflict(format!(
                        "task '{}' is already finished and cannot be cancelled",
                        task_id
                    )));
                }

                update(tasks::table.filter(tasks::id.eq(task_id)))
                    .set((
                        tasks::status.eq("cancelled"),
                        tasks::finished_at.eq(diesel::dsl::now),
                        tasks::updated_at.eq(diesel::dsl::now),
                    ))
                    .returning(TaskRow::as_returning())
                    .get_result::<TaskRow>(connection)?
                    .into_domain()
            })
            .await
    }

    async fn purge_finished_tasks_before(&self, cutoff: DateTime<Utc>) -> Result<usize, AppError> {
        self.database
            .run(move |connection| {
                delete(
                    tasks::table
                        .filter(tasks::status.eq_any(["succeeded", "failed", "cancelled"]))
                        .filter(tasks::finished_at.is_not_null())
                        .filter(tasks::finished_at.lt(cutoff)),
                )
                .execute(connection)
                .map_err(AppError::internal)
            })
            .await
    }
}
