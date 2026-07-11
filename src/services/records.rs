use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::RecordFilter,
        pagination::{Page, PageRequest},
        resource_records::{
            CreateRecordInstance, CreateRecordTypeDefinition, RecordInstance, RecordRrset,
            RecordTypeDefinition, UpdateRecord,
        },
        types::RecordTypeName,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, RecordStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "record_type"))]
pub async fn list_types(
    store: &(dyn RecordStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<RecordTypeDefinition>, AppError> {
    store.list_record_types(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "record_type"))]
pub async fn create_type(
    storage: &DynStorage,
    command: CreateRecordTypeDefinition,
    events: &EventSinkClient,
) -> Result<RecordTypeDefinition, AppError> {
    let (record_type, history) = storage
        .transaction(move |tx| {
            let record_type = tx.records().create_record_type(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "record_type",
                Some(record_type.id()),
                record_type.name().as_str(),
                actions::CREATE,
                json!({"name": record_type.name().as_str()}),
            ))?;
            Ok((record_type, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(record_type)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "record"))]
pub async fn list_records(
    store: &(dyn RecordStore + Send + Sync),
    page: &PageRequest,
    filter: &RecordFilter,
) -> Result<Page<RecordInstance>, AppError> {
    store.list_records(page, filter).await
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "rrset"))]
pub async fn list_rrsets(
    store: &(dyn RecordStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<RecordRrset>, AppError> {
    store.list_rrsets(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "record"))]
pub async fn create_record(
    storage: &DynStorage,
    command: CreateRecordInstance,
    events: &EventSinkClient,
) -> Result<RecordInstance, AppError> {
    let (record, history) = storage
        .transaction(move |tx| {
            let record = tx.records().create_record(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "record",
                Some(record.id()),
                record.owner_name(),
                actions::CREATE,
                json!({
                    "type_name": record.type_name().as_str(),
                    "owner_name": record.owner_name(),
                    "rrset_id": record.rrset_id().to_string(),
                }),
            ))?;
            Ok((record, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(record)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "record"))]
pub async fn get_record(
    store: &(dyn RecordStore + Send + Sync),
    record_id: uuid::Uuid,
) -> Result<RecordInstance, AppError> {
    store.get_record(record_id).await
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "rrset"))]
pub async fn get_rrset(
    store: &(dyn RecordStore + Send + Sync),
    rrset_id: uuid::Uuid,
) -> Result<RecordRrset, AppError> {
    store.get_rrset(rrset_id).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "record"))]
pub async fn delete_record(
    storage: &DynStorage,
    record_id: uuid::Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx.records().get_record(record_id)?;
            tx.records().delete_record(record_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "record",
                Some(old.id()),
                old.owner_name(),
                actions::DELETE,
                json!({
                    "type_name": old.type_name().as_str(),
                    "owner_name": old.owner_name(),
                    "rrset_id": old.rrset_id().to_string(),
                }),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "rrset"))]
pub async fn delete_rrset(
    storage: &DynStorage,
    rrset_id: uuid::Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx.records().get_rrset(rrset_id)?;
            tx.records().delete_rrset(rrset_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "rrset",
                Some(old.id()),
                old.owner_name().as_str(),
                actions::DELETE,
                json!({
                    "type_name": old.type_name().as_str(),
                    "owner_name": old.owner_name().as_str(),
                }),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "record"))]
pub async fn update_record(
    storage: &DynStorage,
    record_id: uuid::Uuid,
    command: UpdateRecord,
    events: &EventSinkClient,
) -> Result<RecordInstance, AppError> {
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.records().get_record(record_id)?;
            let new = tx.records().update_record(record_id, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "record",
                Some(new.id()),
                new.owner_name(),
                actions::UPDATE,
                json!({
                    "type_name": new.type_name().as_str(),
                    "owner_name": new.owner_name(),
                    "old": {"data": old.data(), "ttl": old.ttl().map(|t| t.as_u32())},
                    "new": {"data": new.data(), "ttl": new.ttl().map(|t| t.as_u32())},
                }),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "record_type"))]
pub async fn delete_record_type(
    storage: &DynStorage,
    name: &RecordTypeName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            tx.records().delete_record_type(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "record_type",
                None,
                name_owned.as_str(),
                actions::DELETE,
                json!({"name": name_owned.as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
