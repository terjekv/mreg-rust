use serde_json::json;

use crate::{
    audit::actions,
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

    super::audit_mutation(
        audit,
        events,
        "host_group",
        actions::CREATE,
        Some(group.id()),
        group.name().as_str(),
        json!({"name": group.name().as_str(), "description": group.description()}),
    )
    .await;

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

    super::audit_mutation(
        audit,
        events,
        "host_group",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str(), "description": old.description()}),
    )
    .await;

    Ok(())
}
