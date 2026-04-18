use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        filters::HostFilter,
        host::{
            AssignIpAddress, CreateHost, Host, HostAuthContext, IpAddressAssignment, UpdateHost,
            UpdateIpAddress,
        },
        pagination::{Page, PageRequest},
        types::{Hostname, IpAddressValue},
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, HostStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host"))]
pub async fn list(
    store: &(dyn HostStore + Send + Sync),
    page: &PageRequest,
    filter: &HostFilter,
) -> Result<Page<Host>, AppError> {
    store.list_hosts(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host"))]
pub async fn create(
    store: &(dyn HostStore + Send + Sync),
    command: CreateHost,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<Host, AppError> {
    let host = store.create_host(command).await?;

    super::audit_mutation(
        audit,
        events,
        "host",
        actions::CREATE,
        Some(host.id()),
        host.name().as_str(),
        json!({"name": host.name().as_str(), "comment": host.comment()}),
    )
    .await;

    Ok(host)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host"))]
pub async fn get(store: &(dyn HostStore + Send + Sync), name: &Hostname) -> Result<Host, AppError> {
    store.get_host_by_name(name).await
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host"))]
pub async fn get_auth_context(
    store: &(dyn HostStore + Send + Sync),
    name: &Hostname,
) -> Result<HostAuthContext, AppError> {
    store.get_host_auth_context(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host"))]
pub async fn update(
    store: &(dyn HostStore + Send + Sync),
    name: &Hostname,
    command: UpdateHost,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<Host, AppError> {
    let old = store.get_host_by_name(name).await?;
    let new = store.update_host(name, command).await?;

    super::audit_mutation(
        audit,
        events,
        "host",
        actions::UPDATE,
        Some(new.id()),
        new.name().as_str(),
        json!({
            "old": {"name": old.name().as_str(), "comment": old.comment()},
            "new": {"name": new.name().as_str(), "comment": new.comment()}
        }),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host"))]
pub async fn delete(
    store: &(dyn HostStore + Send + Sync),
    name: &Hostname,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let old = store.get_host_by_name(name).await?;
    store.delete_host(name).await?;

    super::audit_mutation(
        audit,
        events,
        "host",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str()}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "ip_address"))]
pub async fn list_ip_addresses(
    store: &(dyn HostStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<IpAddressAssignment>, AppError> {
    store.list_ip_addresses(page).await
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "ip_address"))]
pub async fn list_host_ip_addresses(
    store: &(dyn HostStore + Send + Sync),
    name: &Hostname,
    page: &PageRequest,
) -> Result<Page<IpAddressAssignment>, AppError> {
    store.list_ip_addresses_for_host(name, page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "ip_address"))]
pub async fn assign_ip_address(
    store: &(dyn HostStore + Send + Sync),
    command: AssignIpAddress,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<IpAddressAssignment, AppError> {
    let assignment = store.assign_ip_address(command).await?;

    super::audit_mutation(
        audit,
        events,
        "ip_address",
        actions::CREATE,
        Some(assignment.id()),
        assignment.address().as_str(),
        json!({"host_id": assignment.host_id(), "address": assignment.address().as_str()}),
    )
    .await;

    Ok(assignment)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "ip_address"))]
pub async fn update_ip_address(
    store: &(dyn HostStore + Send + Sync),
    address: &IpAddressValue,
    command: UpdateIpAddress,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<IpAddressAssignment, AppError> {
    let old = store.get_ip_address(address).await?;
    let old_mac: Option<String> = old.mac_address().map(|m| m.as_str());

    let updated = store.update_ip_address(address, command).await?;
    let new_mac = updated.mac_address().map(|m| m.as_str());

    super::audit_mutation(
        audit,
        events,
        "ip_address",
        actions::UPDATE,
        Some(updated.id()),
        updated.address().as_str(),
        json!({
            "old": {"mac_address": old_mac},
            "new": {"mac_address": new_mac}
        }),
    )
    .await;

    Ok(updated)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "ip_address"))]
pub async fn unassign_ip_address(
    store: &(dyn HostStore + Send + Sync),
    address: &IpAddressValue,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let old = store.unassign_ip_address(address).await?;

    super::audit_mutation(
        audit,
        events,
        "ip_address",
        actions::DELETE,
        Some(old.id()),
        old.address().as_str(),
        json!({"address": old.address().as_str(), "host_id": old.host_id()}),
    )
    .await;

    Ok(())
}
