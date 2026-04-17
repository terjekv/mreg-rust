use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{Page, PageRequest},
        types::LabelName,
    },
    errors::AppError,
    storage::LabelStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

pub(super) fn create_label_in_state(
    state: &mut MemoryState,
    command: CreateLabel,
) -> Result<Label, AppError> {
    let key = command.name().as_str().to_string();
    if state.labels.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "label '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let label = Label::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description().to_string(),
        now,
        now,
    )?;
    state.labels.insert(key, label.clone());
    Ok(label)
}

#[async_trait]
impl LabelStore for MemoryStorage {
    async fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<Label> = state.labels.values().cloned().collect();
        sort_items(&mut items, page, |label, field| match field {
            "description" => label.description().to_string(),
            "created_at" => label.created_at().to_rfc3339(),
            _ => label.name().as_str().to_string(),
        });
        paginate_by_cursor(items, page)
    }

    async fn create_label(&self, command: CreateLabel) -> Result<Label, AppError> {
        let mut state = self.state.write().await;
        let label = create_label_in_state(&mut state, command)?;
        Ok(label)
    }

    async fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError> {
        let state = self.state.read().await;
        state
            .labels
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("label '{}' was not found", name.as_str())))
    }

    async fn update_label(
        &self,
        name: &LabelName,
        command: UpdateLabel,
    ) -> Result<Label, AppError> {
        let mut state = self.state.write().await;
        let label = state.labels.get(name.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("label '{}' was not found", name.as_str()))
        })?;
        let now = Utc::now();
        let description = command
            .description
            .unwrap_or_else(|| label.description().to_string());
        let updated = Label::restore(
            label.id(),
            label.name().clone(),
            description,
            label.created_at(),
            now,
        )?;
        state
            .labels
            .insert(name.as_str().to_string(), updated.clone());
        Ok(updated)
    }

    async fn delete_label(&self, name: &LabelName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        match state.labels.remove(name.as_str()) {
            Some(_removed) => Ok(()),
            None => Err(AppError::not_found(format!(
                "label '{}' was not found",
                name.as_str()
            ))),
        }
    }
}
