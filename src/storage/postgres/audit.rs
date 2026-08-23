use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    ExpressionMethods, PgConnection, QueryDsl, QueryableByName, RunQueryDsl, SelectableHelper,
    insert_into, sql_query,
    sql_types::{Integer, Jsonb, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent, OutboxClaim},
    db::{models::HistoryEventRow, schema::history_events},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
    storage::{AuditStore, OutboxStore},
};

use super::PostgresStorage;
use super::helpers::vec_to_page_by;

#[derive(QueryableByName)]
struct OutboxClaimRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    actor: String,
    #[diesel(sql_type = Text)]
    resource_kind: String,
    #[diesel(sql_type = Nullable<SqlUuid>)]
    resource_id: Option<Uuid>,
    #[diesel(sql_type = Text)]
    resource_name: String,
    #[diesel(sql_type = Text)]
    action: String,
    #[diesel(sql_type = Jsonb)]
    data: Value,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = SqlUuid)]
    delivery_lease_id: Uuid,
    #[diesel(sql_type = Integer)]
    delivery_attempts: i32,
}

impl OutboxClaimRow {
    fn into_domain(self) -> OutboxClaim {
        OutboxClaim::new(
            HistoryEvent::restore(
                self.id,
                self.actor,
                self.resource_kind,
                self.resource_id,
                self.resource_name,
                self.action,
                self.data,
                self.created_at,
            ),
            self.delivery_lease_id,
            self.delivery_attempts.max(0) as u32,
        )
    }
}

pub(super) fn record_event_in_conn(
    connection: &mut PgConnection,
    event: CreateHistoryEvent,
) -> Result<HistoryEvent, AppError> {
    let actor = event.actor().to_string();
    let resource_kind = event.resource_kind().to_string();
    let resource_id = event.resource_id();
    let resource_name = event.resource_name().to_string();
    let action = event.action().to_string();
    let data = event.data().clone();

    let row = insert_into(history_events::table)
        .values((
            history_events::actor.eq(&actor),
            history_events::resource_kind.eq(&resource_kind),
            history_events::resource_id.eq(resource_id),
            history_events::resource_name.eq(&resource_name),
            history_events::action.eq(&action),
            history_events::data.eq(&data),
        ))
        .returning(HistoryEventRow::as_returning())
        .get_result(connection)?;

    Ok(row.into_domain())
}

pub(super) fn list_events_in_conn(
    connection: &mut PgConnection,
    page: &PageRequest,
) -> Result<Page<HistoryEvent>, AppError> {
    let rows = history_events::table
        .select(HistoryEventRow::as_select())
        .order(history_events::created_at.desc())
        .load::<HistoryEventRow>(connection)?;

    let items: Vec<HistoryEvent> = rows.into_iter().map(HistoryEventRow::into_domain).collect();
    vec_to_page_by(
        items,
        page,
        "created_at",
        &crate::domain::pagination::SortDirection::Desc,
        |item| item.created_at().to_rfc3339(),
    )
}

#[async_trait]
impl AuditStore for PostgresStorage {
    async fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        self.database
            .run(move |connection| record_event_in_conn(connection, event))
            .await
    }

    async fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        let page = page.clone();
        self.database
            .run(move |connection| list_events_in_conn(connection, &page))
            .await
    }
}

#[async_trait]
impl OutboxStore for PostgresStorage {
    async fn claim_events(&self, limit: u32) -> Result<Vec<OutboxClaim>, AppError> {
        self.database
            .run(move |connection| {
                let lease_id = Uuid::new_v4();
                let rows = sql_query(
                    "WITH candidates AS (
                         SELECT id
                         FROM history_events
                         WHERE delivered_at IS NULL
                           AND delivery_available_at <= now()
                           AND (delivery_lease_until IS NULL OR delivery_lease_until <= now())
                         ORDER BY created_at, id
                         FOR UPDATE SKIP LOCKED
                         LIMIT $1
                     )
                     UPDATE history_events event
                     SET delivery_attempts = event.delivery_attempts + 1,
                         delivery_lease_id = $2,
                         delivery_lease_until = now() + interval '30 seconds'
                     FROM candidates
                     WHERE event.id = candidates.id
                     RETURNING event.id, event.actor, event.resource_kind,
                               event.resource_id, event.resource_name, event.action,
                               event.data, event.created_at, event.delivery_lease_id,
                               event.delivery_attempts",
                )
                .bind::<diesel::sql_types::BigInt, _>(i64::from(limit))
                .bind::<SqlUuid, _>(lease_id)
                .load::<OutboxClaimRow>(connection)?;
                Ok(rows.into_iter().map(OutboxClaimRow::into_domain).collect())
            })
            .await
    }

    async fn complete_event(&self, event_id: Uuid, lease_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                let updated = sql_query(
                    "UPDATE history_events
                     SET delivered_at = now(), delivery_lease_id = NULL,
                         delivery_lease_until = NULL, delivery_error = NULL
                     WHERE id = $1 AND delivery_lease_id = $2 AND delivered_at IS NULL",
                )
                .bind::<SqlUuid, _>(event_id)
                .bind::<SqlUuid, _>(lease_id)
                .execute(connection)?;
                if updated == 0 {
                    return Err(AppError::conflict("event delivery lease was lost"));
                }
                Ok(())
            })
            .await
    }

    async fn retry_event(
        &self,
        event_id: Uuid,
        lease_id: Uuid,
        error: &str,
        delay_seconds: u32,
    ) -> Result<(), AppError> {
        let error = error.chars().take(2048).collect::<String>();
        self.database
            .run(move |connection| {
                let updated = sql_query(
                    "UPDATE history_events
                     SET delivery_available_at = now() + ($3 * interval '1 second'),
                         delivery_lease_id = NULL, delivery_lease_until = NULL,
                         delivery_error = $4
                     WHERE id = $1 AND delivery_lease_id = $2 AND delivered_at IS NULL",
                )
                .bind::<SqlUuid, _>(event_id)
                .bind::<SqlUuid, _>(lease_id)
                .bind::<Integer, _>(i32::try_from(delay_seconds).unwrap_or(i32::MAX))
                .bind::<Text, _>(&error)
                .execute(connection)?;
                if updated == 0 {
                    return Err(AppError::conflict("event delivery lease was lost"));
                }
                Ok(())
            })
            .await
    }
}
