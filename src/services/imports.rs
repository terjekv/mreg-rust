use uuid::Uuid;

use crate::{
    domain::{
        imports::{CreateImportBatch, ImportBatchSummary},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
    storage::ImportStore,
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "import_batch"))]
pub async fn list(
    store: &(dyn ImportStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ImportBatchSummary>, AppError> {
    store.list_import_batches(page).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "import_batch"))]
pub async fn create(
    store: &(dyn ImportStore + Send + Sync),
    command: CreateImportBatch,
) -> Result<ImportBatchSummary, AppError> {
    store.create_import_batch(command).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "import_batch"))]
pub async fn run(
    store: &(dyn ImportStore + Send + Sync),
    import_id: Uuid,
) -> Result<ImportBatchSummary, AppError> {
    store.run_import_batch(import_id).await
}
