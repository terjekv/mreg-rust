use async_trait::async_trait;

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
};

/// Immutable audit trail storage.
#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError>;
    async fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError>;
}
