use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest},
        types::BacnetIdentifier,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{BacnetStore, DynStorage},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "bacnet_id"))]
pub async fn list_bacnet_ids(
    store: &(dyn BacnetStore + Send + Sync),
    page: &PageRequest,
    filter: &BacnetIdFilter,
) -> Result<Page<BacnetIdAssignment>, AppError> {
    store.list_bacnet_ids(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "bacnet_id"))]
pub async fn create_bacnet_id(
    storage: &DynStorage,
    command: CreateBacnetIdAssignment,
    events: &EventSinkClient,
) -> Result<BacnetIdAssignment, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            let item = tx.bacnet().create_bacnet_id(command)?;
            let bid_str = item.bacnet_id().as_u32().to_string();
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "bacnet_id",
                None,
                bid_str,
                actions::CREATE,
                json!({"bacnet_id": item.bacnet_id().as_u32(), "host_name": item.host_name().as_str()}),
            ))?;
            Ok((item, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "bacnet_id"))]
pub async fn get_bacnet_id(
    store: &(dyn BacnetStore + Send + Sync),
    bacnet_id: BacnetIdentifier,
) -> Result<BacnetIdAssignment, AppError> {
    store.get_bacnet_id(bacnet_id).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "bacnet_id"))]
pub async fn delete_bacnet_id(
    storage: &DynStorage,
    bacnet_id: BacnetIdentifier,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx.bacnet().get_bacnet_id(bacnet_id)?;
            tx.bacnet().delete_bacnet_id(bacnet_id)?;
            let bid_str = old.bacnet_id().as_u32().to_string();
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "bacnet_id",
                None,
                bid_str,
                actions::DELETE,
                json!({"bacnet_id": old.bacnet_id().as_u32(), "host_name": old.host_name().as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
