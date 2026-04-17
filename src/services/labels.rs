use serde_json::json;

use crate::{
    audit::CreateHistoryEvent,
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, LabelStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "label"))]
pub async fn list(
    store: &(dyn LabelStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<Label>, AppError> {
    store.list_labels(page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "label"))]
pub async fn create(
    store: &(dyn LabelStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateLabel,
) -> Result<Label, AppError> {
    let label = store.create_label(command).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "label",
        Some(label.id()),
        label.name().as_str(),
        "create",
        json!({"name": label.name().as_str(), "description": label.description()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(label)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "label"))]
pub async fn get(
    store: &(dyn LabelStore + Send + Sync),
    name: &crate::domain::types::LabelName,
) -> Result<Label, AppError> {
    store.get_label_by_name(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "label"))]
pub async fn update(
    store: &(dyn LabelStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &crate::domain::types::LabelName,
    command: UpdateLabel,
) -> Result<Label, AppError> {
    let old = store.get_label_by_name(name).await?;
    let new = store.update_label(name, command).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "label",
        Some(new.id()),
        new.name().as_str(),
        "update",
        json!({"old": {"description": old.description()}, "new": {"description": new.description()}}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "label"))]
pub async fn delete(
    store: &(dyn LabelStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &crate::domain::types::LabelName,
) -> Result<(), AppError> {
    let old = store.get_label_by_name(name).await?;
    store.delete_label(name).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "label",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str(), "description": old.description()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(())
}
