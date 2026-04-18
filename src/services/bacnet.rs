use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest},
        types::BacnetIdentifier,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, BacnetStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "bacnet_id"))]
pub async fn list_bacnet_ids(
    store: &(dyn BacnetStore + Send + Sync),
    page: &PageRequest,
    filter: &BacnetIdFilter,
) -> Result<Page<BacnetIdAssignment>, AppError> {
    store.list_bacnet_ids(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "bacnet_id"))]
pub async fn create_bacnet_id(
    store: &(dyn BacnetStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateBacnetIdAssignment,
) -> Result<BacnetIdAssignment, AppError> {
    let item = store.create_bacnet_id(command).await?;

    let bid_str = item.bacnet_id().as_u32().to_string();
    super::audit_mutation(
        audit,
        events,
        "bacnet_id",
        actions::CREATE,
        None,
        &bid_str,
        json!({"bacnet_id": item.bacnet_id().as_u32(), "host_name": item.host_name().as_str()}),
    )
    .await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "bacnet_id"))]
pub async fn get_bacnet_id(
    store: &(dyn BacnetStore + Send + Sync),
    bacnet_id: BacnetIdentifier,
) -> Result<BacnetIdAssignment, AppError> {
    store.get_bacnet_id(bacnet_id).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "bacnet_id"))]
pub async fn delete_bacnet_id(
    store: &(dyn BacnetStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    bacnet_id: BacnetIdentifier,
) -> Result<(), AppError> {
    let old = store.get_bacnet_id(bacnet_id).await?;
    store.delete_bacnet_id(bacnet_id).await?;

    let bid_str = old.bacnet_id().as_u32().to_string();
    super::audit_mutation(
        audit,
        events,
        "bacnet_id",
        actions::DELETE,
        None,
        &bid_str,
        json!({"bacnet_id": old.bacnet_id().as_u32(), "host_name": old.host_name().as_str()}),
    )
    .await;

    Ok(())
}
