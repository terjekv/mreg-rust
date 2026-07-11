use crate::{
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{Page, PageRequest},
        types::LabelName,
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::LabelStore`].
pub trait TxLabelStore {
    fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError>;
    fn create_label(&self, command: CreateLabel) -> Result<Label, AppError>;
    fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError>;
    fn update_label(&self, name: &LabelName, command: UpdateLabel) -> Result<Label, AppError>;
    fn delete_label(&self, name: &LabelName) -> Result<(), AppError>;
}
