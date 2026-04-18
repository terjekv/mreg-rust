use serde_json::json;

use crate::{
    audit::actions,
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
    events::EventSinkClient,
    storage::{AuditStore, RecordStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "record_type"))]
pub async fn list_types(
    store: &(dyn RecordStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<RecordTypeDefinition>, AppError> {
    store.list_record_types(page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "record_type"))]
pub async fn create_type(
    store: &(dyn RecordStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateRecordTypeDefinition,
) -> Result<RecordTypeDefinition, AppError> {
    let record_type = store.create_record_type(command).await?;

    super::audit_mutation(
        audit,
        events,
        "record_type",
        actions::CREATE,
        Some(record_type.id()),
        record_type.name().as_str(),
        json!({"name": record_type.name().as_str()}),
    )
    .await;

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

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "record"))]
pub async fn create_record(
    store: &(dyn RecordStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateRecordInstance,
) -> Result<RecordInstance, AppError> {
    let record = store.create_record(command).await?;

    super::audit_mutation(
        audit,
        events,
        "record",
        actions::CREATE,
        Some(record.id()),
        record.owner_name(),
        json!({
            "type_name": record.type_name().as_str(),
            "owner_name": record.owner_name(),
            "rrset_id": record.rrset_id().to_string(),
        }),
    )
    .await;

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

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "record"))]
pub async fn delete_record(
    store: &(dyn RecordStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    record_id: uuid::Uuid,
) -> Result<(), AppError> {
    let old = store.get_record(record_id).await?;
    store.delete_record(record_id).await?;

    super::audit_mutation(
        audit,
        events,
        "record",
        actions::DELETE,
        Some(old.id()),
        old.owner_name(),
        json!({
            "type_name": old.type_name().as_str(),
            "owner_name": old.owner_name(),
            "rrset_id": old.rrset_id().to_string(),
        }),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "rrset"))]
pub async fn delete_rrset(
    store: &(dyn RecordStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    rrset_id: uuid::Uuid,
) -> Result<(), AppError> {
    let old = store.get_rrset(rrset_id).await?;
    store.delete_rrset(rrset_id).await?;

    super::audit_mutation(
        audit,
        events,
        "rrset",
        actions::DELETE,
        Some(old.id()),
        old.owner_name().as_str(),
        json!({
            "type_name": old.type_name().as_str(),
            "owner_name": old.owner_name().as_str(),
        }),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "record"))]
pub async fn update_record(
    store: &(dyn RecordStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    record_id: uuid::Uuid,
    command: UpdateRecord,
) -> Result<RecordInstance, AppError> {
    let old = store.get_record(record_id).await?;
    let new = store.update_record(record_id, command).await?;

    super::audit_mutation(
        audit,
        events,
        "record",
        actions::UPDATE,
        Some(new.id()),
        new.owner_name(),
        json!({
            "type_name": new.type_name().as_str(),
            "owner_name": new.owner_name(),
            "old": {"data": old.data(), "ttl": old.ttl().map(|t| t.as_u32())},
            "new": {"data": new.data(), "ttl": new.ttl().map(|t| t.as_u32())},
        }),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "record_type"))]
pub async fn delete_record_type(
    store: &(dyn RecordStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &RecordTypeName,
) -> Result<(), AppError> {
    store.delete_record_type(name).await?;

    super::audit_mutation(
        audit,
        events,
        "record_type",
        actions::DELETE,
        None,
        name.as_str(),
        json!({"name": name.as_str()}),
    )
    .await;

    Ok(())
}
