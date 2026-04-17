use serde_json::json;

use crate::{
    audit::CreateHistoryEvent,
    domain::{
        filters::HostGroupFilter,
        host_group::{CreateHostGroup, HostGroup},
        pagination::{Page, PageRequest},
        types::HostGroupName,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, HostGroupStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_group"))]
pub async fn list_host_groups(
    store: &(dyn HostGroupStore + Send + Sync),
    page: &PageRequest,
    filter: &HostGroupFilter,
) -> Result<Page<HostGroup>, AppError> {
    store.list_host_groups(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_group"))]
pub async fn create_host_group(
    store: &(dyn HostGroupStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateHostGroup,
) -> Result<HostGroup, AppError> {
    let group = store.create_host_group(command).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host_group",
        Some(group.id()),
        group.name().as_str(),
        "create",
        json!({"name": group.name().as_str(), "description": group.description()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(group)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_group"))]
pub async fn get_host_group(
    store: &(dyn HostGroupStore + Send + Sync),
    name: &HostGroupName,
) -> Result<HostGroup, AppError> {
    store.get_host_group_by_name(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_group"))]
pub async fn delete_host_group(
    store: &(dyn HostGroupStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &HostGroupName,
) -> Result<(), AppError> {
    let old = store.get_host_group_by_name(name).await?;
    store.delete_host_group(name).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host_group",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str(), "description": old.description()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(())
}
