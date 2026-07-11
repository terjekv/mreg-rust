use crate::{
    audit::{CreateHistoryEvent, HistoryEvent},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::AuditStore`].
///
/// No `Send`/`Sync` bound: the trait object lives only for the duration of the
/// transaction closure, which runs single-threaded (one `spawn_blocking` worker
/// for Postgres; under the write lock for Memory).
pub trait TxAuditStore {
    fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError>;
    fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError>;
}
