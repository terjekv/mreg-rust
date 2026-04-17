use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
    storage::AuditStore,
};

use super::{MemoryStorage, paginate_by_cursor};

#[async_trait]
impl AuditStore for MemoryStorage {
    async fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        let mut state = self.state.write().await;
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
        Ok(history_event)
    }

    async fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<HistoryEvent> = state.history_events.clone();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }
}
