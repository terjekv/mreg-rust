use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
    storage::AuditStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor};

pub(super) fn record_event_in_state(
    state: &mut MemoryState,
    event: CreateHistoryEvent,
) -> HistoryEvent {
    let now = Utc::now();
    let history_event = HistoryEvent::restore(
        Uuid::new_v4(),
        event.actor().to_string(),
        event.resource_kind().to_string(),
        event.resource_id(),
        event.resource_name().to_string(),
        event.action().to_string(),
        event.data().clone(),
        now,
    );
    state.history_events.push(history_event.clone());
    history_event
}

pub(super) fn list_events_in_state(
    state: &MemoryState,
    page: &PageRequest,
) -> Result<Page<HistoryEvent>, AppError> {
    let mut items: Vec<HistoryEvent> = state.history_events.clone();
    items.sort_by_key(|item| item.id());
    paginate_by_cursor(items, page)
}

#[async_trait]
impl AuditStore for MemoryStorage {
    async fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        let mut state = self.state.write().await;
        Ok(record_event_in_state(&mut state, event))
    }

    async fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        let state = self.state.read().await;
        list_events_in_state(&state, page)
    }
}
