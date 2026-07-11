use async_trait::async_trait;
use diesel::{
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper, insert_into,
};

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent},
    db::{models::HistoryEventRow, schema::history_events},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
    storage::AuditStore,
};

use super::PostgresStorage;
use super::helpers::vec_to_page;

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
    Ok(vec_to_page(items, page))
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
