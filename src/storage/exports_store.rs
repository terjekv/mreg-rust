use async_trait::async_trait;

use crate::{
    domain::{
        exports::{CreateExportRun, CreateExportTemplate, ExportRun, ExportTemplate},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
};

/// Export template and run operations.
#[async_trait]
pub trait ExportStore: Send + Sync {
    async fn list_export_templates(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ExportTemplate>, AppError>;
    async fn list_export_runs(&self, page: &PageRequest) -> Result<Page<ExportRun>, AppError>;
    async fn create_export_template(
        &self,
        command: CreateExportTemplate,
    ) -> Result<ExportTemplate, AppError>;
    async fn create_export_run(&self, command: CreateExportRun) -> Result<ExportRun, AppError>;
    async fn run_export(&self, run_id: uuid::Uuid) -> Result<ExportRun, AppError>;
}
