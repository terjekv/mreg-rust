use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        filters::PtrOverrideFilter,
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        types::IpAddressValue,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, PtrOverrideStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "ptr_override"))]
pub async fn list_ptr_overrides(
    store: &(dyn PtrOverrideStore + Send + Sync),
    page: &PageRequest,
    filter: &PtrOverrideFilter,
) -> Result<Page<PtrOverride>, AppError> {
    store.list_ptr_overrides(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "ptr_override"))]
pub async fn create_ptr_override(
    store: &(dyn PtrOverrideStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreatePtrOverride,
) -> Result<PtrOverride, AppError> {
    let item = store.create_ptr_override(command).await?;

    super::audit_mutation(
        audit,
        events,
        "ptr_override",
        actions::CREATE,
        Some(item.id()),
        item.address().as_str(),
        json!({"host_name": item.host_name().as_str(), "address": item.address().as_str()}),
    )
    .await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "ptr_override"))]
pub async fn get_ptr_override(
    store: &(dyn PtrOverrideStore + Send + Sync),
    address: &IpAddressValue,
) -> Result<PtrOverride, AppError> {
    store.get_ptr_override_by_address(address).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "ptr_override"))]
pub async fn delete_ptr_override(
    store: &(dyn PtrOverrideStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    address: &IpAddressValue,
) -> Result<(), AppError> {
    let old = store.get_ptr_override_by_address(address).await?;
    store.delete_ptr_override(address).await?;

    super::audit_mutation(
        audit,
        events,
        "ptr_override",
        actions::DELETE,
        Some(old.id()),
        old.address().as_str(),
        json!({"host_name": old.host_name().as_str(), "address": old.address().as_str()}),
    )
    .await;

    Ok(())
}
