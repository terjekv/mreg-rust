use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        filters::NetworkFilter,
        host::IpAddressAssignment,
        network::{CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork},
        pagination::{Page, PageRequest},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, NetworkStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network"))]
pub async fn list(
    store: &(dyn NetworkStore + Send + Sync),
    page: &PageRequest,
    filter: &NetworkFilter,
) -> Result<Page<Network>, AppError> {
    store.list_networks(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "network"))]
pub async fn create(
    store: &(dyn NetworkStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateNetwork,
) -> Result<Network, AppError> {
    let network = store.create_network(command).await?;

    super::audit_mutation(
        audit,
        events,
        "network",
        actions::CREATE,
        Some(network.id()),
        network.cidr().as_str(),
        json!({
            "cidr": network.cidr().as_str(),
            "description": network.description(),
        }),
    )
    .await;

    Ok(network)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network"))]
pub async fn get(
    store: &(dyn NetworkStore + Send + Sync),
    cidr: &CidrValue,
) -> Result<Network, AppError> {
    store.get_network_by_cidr(cidr).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "network"))]
pub async fn delete(
    store: &(dyn NetworkStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    cidr: &CidrValue,
) -> Result<(), AppError> {
    let old = store.get_network_by_cidr(cidr).await?;
    store.delete_network(cidr).await?;

    super::audit_mutation(
        audit,
        events,
        "network",
        actions::DELETE,
        Some(old.id()),
        old.cidr().as_str(),
        json!({
            "cidr": old.cidr().as_str(),
            "description": old.description(),
        }),
    )
    .await;

    Ok(())
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "excluded_range"))]
pub async fn list_excluded_ranges(
    store: &(dyn NetworkStore + Send + Sync),
    cidr: &CidrValue,
    page: &PageRequest,
) -> Result<Page<ExcludedRange>, AppError> {
    store.list_excluded_ranges(cidr, page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "network"))]
pub async fn update(
    store: &(dyn NetworkStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    cidr: &CidrValue,
    command: UpdateNetwork,
) -> Result<Network, AppError> {
    let old = store.get_network_by_cidr(cidr).await?;
    let new = store.update_network(cidr, command).await?;

    super::audit_mutation(
        audit,
        events,
        "network",
        actions::UPDATE,
        Some(new.id()),
        new.cidr().as_str(),
        json!({
            "old": {"description": old.description(), "vlan": old.vlan(), "frozen": old.frozen(), "reserved": old.reserved()},
            "new": {"description": new.description(), "vlan": new.vlan(), "frozen": new.frozen(), "reserved": new.reserved()},
        }),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "excluded_range"))]
pub async fn add_excluded_range(
    store: &(dyn NetworkStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    cidr: &CidrValue,
    command: CreateExcludedRange,
) -> Result<ExcludedRange, AppError> {
    let range = store.add_excluded_range(cidr, command).await?;

    let range_name = format!("{}-{}", range.start_ip().as_str(), range.end_ip().as_str());
    super::audit_mutation(
        audit,
        events,
        "excluded_range",
        actions::CREATE,
        Some(range.id()),
        &range_name,
        json!({
            "start_ip": range.start_ip().as_str(),
            "end_ip": range.end_ip().as_str(),
            "description": range.description(),
        }),
    )
    .await;

    Ok(range)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network"))]
pub async fn list_used_addresses(
    store: &(dyn NetworkStore + Send + Sync),
    cidr: &CidrValue,
) -> Result<Vec<IpAddressAssignment>, AppError> {
    store.list_used_addresses(cidr).await
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network"))]
pub async fn list_unused_addresses(
    store: &(dyn NetworkStore + Send + Sync),
    cidr: &CidrValue,
    limit: Option<u32>,
) -> Result<Vec<IpAddressValue>, AppError> {
    store.list_unused_addresses(cidr, limit).await
}
