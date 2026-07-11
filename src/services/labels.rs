use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{Page, PageRequest},
        types::LabelName,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, LabelStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "label"))]
pub async fn list(
    store: &(dyn LabelStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<Label>, AppError> {
    store.list_labels(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "label"))]
pub async fn create(
    storage: &DynStorage,
    command: CreateLabel,
    events: &EventSinkClient,
) -> Result<Label, AppError> {
    let (label, history) = storage
        .transaction(move |tx| {
            let label = tx.labels().create_label(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "label",
                Some(label.id()),
                label.name().as_str(),
                actions::CREATE,
                json!({"name": label.name().as_str(), "description": label.description()}),
            ))?;
            Ok((label, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(label)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "label"))]
pub async fn get(
    store: &(dyn LabelStore + Send + Sync),
    name: &LabelName,
) -> Result<Label, AppError> {
    store.get_label_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "label"))]
pub async fn update(
    storage: &DynStorage,
    name: &LabelName,
    command: UpdateLabel,
    events: &EventSinkClient,
) -> Result<Label, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.labels().get_label_by_name(&name_owned)?;
            let new = tx.labels().update_label(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "label",
                Some(new.id()),
                new.name().as_str(),
                actions::UPDATE,
                json!({
                    "old": {"description": old.description()},
                    "new": {"description": new.description()}
                }),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "label"))]
pub async fn delete(
    storage: &DynStorage,
    name: &LabelName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.labels().get_label_by_name(&name_owned)?;
            tx.labels().delete_label(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "label",
                Some(old.id()),
                old.name().as_str(),
                actions::DELETE,
                json!({"name": old.name().as_str(), "description": old.description()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
