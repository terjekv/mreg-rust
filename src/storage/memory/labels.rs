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

use super::{MemoryState, MemoryStorage, sort_and_paginate};

pub(super) fn list_labels_in_state(
    state: &MemoryState,
    page: &PageRequest,
) -> Result<Page<Label>, AppError> {
    let items: Vec<Label> = state.labels.values().cloned().collect();
    sort_and_paginate(
        items,
        page,
        &["name", "description", "created_at"],
        |label, field| match field {
            "description" => label.description().to_string(),
            "created_at" => label.created_at().to_rfc3339(),
            _ => label.name().as_str().to_string(),
        },
    )
}

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

pub(super) fn get_label_by_name_in_state(
    state: &MemoryState,
    name: &LabelName,
) -> Result<Label, AppError> {
    state
        .labels
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| AppError::not_found(format!("label '{}' was not found", name.as_str())))
}

pub(super) fn update_label_in_state(
    state: &mut MemoryState,
    name: &LabelName,
    command: UpdateLabel,
) -> Result<Label, AppError> {
    let label = state.labels.get(name.as_str()).cloned().ok_or_else(|| {
        AppError::not_found(format!("label '{}' was not found", name.as_str()))
    })?;
    let now = Utc::now();
    let new_name = command.name.unwrap_or_else(|| label.name().clone());
    if new_name != *label.name() && state.labels.contains_key(new_name.as_str()) {
        return Err(AppError::conflict(format!(
            "label '{}' already exists",
            new_name.as_str()
        )));
    }
    let description = command
        .description
        .unwrap_or_else(|| label.description().to_string());
    let updated = Label::restore(
        label.id(),
        new_name.clone(),
        description,
        label.created_at(),
        now,
    )?;
    state.labels.remove(name.as_str());
    state
        .labels
        .insert(new_name.as_str().to_string(), updated.clone());
    if new_name != *name {
        for role in state.host_policy_roles.values_mut() {
            if !role
                .labels()
                .iter()
                .any(|label_name| label_name == name.as_str())
            {
                continue;
            }
            let labels = role
                .labels()
                .iter()
                .map(|label_name| {
                    if label_name == name.as_str() {
                        new_name.as_str().to_string()
                    } else {
                        label_name.clone()
                    }
                })
                .collect();
            *role = crate::domain::host_policy::HostPolicyRole::restore(
                role.id(),
                role.name().clone(),
                role.description().to_string(),
                role.atoms().to_vec(),
                role.hosts().to_vec(),
                labels,
                role.created_at(),
                now,
            );
        }
    }
    Ok(updated)
}

pub(super) fn delete_label_in_state(
    state: &mut MemoryState,
    name: &LabelName,
) -> Result<(), AppError> {
    match state.labels.remove(name.as_str()) {
        Some(_removed) => Ok(()),
        None => Err(AppError::not_found(format!(
            "label '{}' was not found",
            name.as_str()
        ))),
    }
}

#[async_trait]
impl LabelStore for MemoryStorage {
    async fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        let state = self.state.read().await;
        list_labels_in_state(&state, page)
    }

    async fn create_label(&self, command: CreateLabel) -> Result<Label, AppError> {
        let mut state = self.state.write().await;
        create_label_in_state(&mut state, command)
    }

    async fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError> {
        let state = self.state.read().await;
        get_label_by_name_in_state(&state, name)
    }

    async fn update_label(
        &self,
        name: &LabelName,
        command: UpdateLabel,
    ) -> Result<Label, AppError> {
        let mut state = self.state.write().await;
        update_label_in_state(&mut state, name, command)
    }

    async fn delete_label(&self, name: &LabelName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_label_in_state(&mut state, name)
    }
}
