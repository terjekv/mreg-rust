use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::PtrOverrideFilter,
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        types::IpAddressValue,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, PtrOverrideStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "ptr_override"))]
pub async fn list_ptr_overrides(
    store: &(dyn PtrOverrideStore + Send + Sync),
    page: &PageRequest,
    filter: &PtrOverrideFilter,
) -> Result<Page<PtrOverride>, AppError> {
    store.list_ptr_overrides(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "ptr_override"))]
pub async fn create_ptr_override(
    storage: &DynStorage,
    command: CreatePtrOverride,
    events: &EventSinkClient,
) -> Result<PtrOverride, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            let item = tx.ptr_overrides().create_ptr_override(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "ptr_override",
                Some(item.id()),
                item.address().as_str(),
                actions::CREATE,
                json!({"host_name": item.host_name().as_str(), "address": item.address().as_str()}),
            ))?;
            Ok((item, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "ptr_override"))]
pub async fn get_ptr_override(
    store: &(dyn PtrOverrideStore + Send + Sync),
    address: &IpAddressValue,
) -> Result<PtrOverride, AppError> {
    store.get_ptr_override_by_address(address).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "ptr_override"))]
pub async fn delete_ptr_override(
    storage: &DynStorage,
    address: &IpAddressValue,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let address_owned = *address;
    let history = storage
        .transaction(move |tx| {
            let old = tx
                .ptr_overrides()
                .get_ptr_override_by_address(&address_owned)?;
            tx.ptr_overrides().delete_ptr_override(&address_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "ptr_override",
                Some(old.id()),
                old.address().as_str(),
                actions::DELETE,
                json!({"host_name": old.host_name().as_str(), "address": old.address().as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
