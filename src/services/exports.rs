use uuid::Uuid;

use crate::{
    domain::{
        exports::{CreateExportRun, CreateExportTemplate, ExportRun, ExportTemplate},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
    storage::ExportStore,
};

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "export_template")
)]
pub async fn list_templates(
    store: &(dyn ExportStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ExportTemplate>, AppError> {
    store.list_export_templates(page).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "export_template"))]
pub async fn create_template(
    store: &(dyn ExportStore + Send + Sync),
    command: CreateExportTemplate,
) -> Result<ExportTemplate, AppError> {
    store.create_export_template(command).await
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "export_run"))]
pub async fn list_runs(
    store: &(dyn ExportStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ExportRun>, AppError> {
    store.list_export_runs(page).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "export_run"))]
pub async fn create_run(
    store: &(dyn ExportStore + Send + Sync),
    command: CreateExportRun,
) -> Result<ExportRun, AppError> {
    store.create_export_run(command).await
}

#[tracing::instrument(skip(store), fields(resource_kind = "export_run"))]
pub async fn run_export(
    store: &(dyn ExportStore + Send + Sync),
    run_id: Uuid,
) -> Result<ExportRun, AppError> {
    store.run_export(run_id).await
}
