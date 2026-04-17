use async_trait::async_trait;

use crate::{
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{Page, PageRequest},
        types::LabelName,
    },
    errors::AppError,
};

/// CRUD operations for labels.
#[async_trait]
pub trait LabelStore: Send + Sync {
    async fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError>;
    async fn create_label(&self, command: CreateLabel) -> Result<Label, AppError>;
    async fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError>;
    async fn update_label(&self, name: &LabelName, command: UpdateLabel)
    -> Result<Label, AppError>;
    async fn delete_label(&self, name: &LabelName) -> Result<(), AppError>;
}
