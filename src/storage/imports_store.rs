use async_trait::async_trait;

use crate::{
    domain::{
        imports::{CreateImportBatch, ImportBatchSummary},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
};

/// Bulk import batch operations.
#[async_trait]
pub trait ImportStore: Send + Sync {
    async fn list_import_batches(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ImportBatchSummary>, AppError>;
    async fn create_import_batch(
        &self,
        command: CreateImportBatch,
    ) -> Result<ImportBatchSummary, AppError>;
    async fn run_import_batch(&self, import_id: uuid::Uuid)
    -> Result<ImportBatchSummary, AppError>;
}
