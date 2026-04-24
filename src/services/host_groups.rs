use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::HostGroupFilter,
        host_group::{CreateHostGroup, HostGroup},
        pagination::{Page, PageRequest},
        types::HostGroupName,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, HostGroupStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_group"))]
pub async fn list_host_groups(
    store: &(dyn HostGroupStore + Send + Sync),
    page: &PageRequest,
    filter: &HostGroupFilter,
) -> Result<Page<HostGroup>, AppError> {
    store.list_host_groups(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_group"))]
pub async fn create_host_group(
    storage: &DynStorage,
    command: CreateHostGroup,
    events: &EventSinkClient,
) -> Result<HostGroup, AppError> {
    let (group, history) = storage
        .transaction(move |tx| {
            let group = tx.host_groups().create_host_group(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_group",
                Some(group.id()),
                group.name().as_str(),
                actions::CREATE,
                json!({"name": group.name().as_str(), "description": group.description()}),
            ))?;
            Ok((group, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(group)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_group"))]
pub async fn get_host_group(
    store: &(dyn HostGroupStore + Send + Sync),
    name: &HostGroupName,
) -> Result<HostGroup, AppError> {
    store.get_host_group_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_group"))]
pub async fn delete_host_group(
    storage: &DynStorage,
    name: &HostGroupName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.host_groups().get_host_group_by_name(&name_owned)?;
            tx.host_groups().delete_host_group(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_group",
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
