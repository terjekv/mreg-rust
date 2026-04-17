use serde_json::json;

use crate::{
    audit::CreateHistoryEvent,
    domain::{
        filters::HostFilter,
        host::{
            AssignIpAddress, CreateHost, Host, IpAddressAssignment, UpdateHost, UpdateIpAddress,
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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host",
        Some(host.id()),
        host.name().as_str(),
        "create",
        json!({"name": host.name().as_str(), "comment": host.comment()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(host)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host"))]
pub async fn get(store: &(dyn HostStore + Send + Sync), name: &Hostname) -> Result<Host, AppError> {
    store.get_host_by_name(name).await
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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host",
        Some(new.id()),
        new.name().as_str(),
        "update",
        json!({
            "old": {"name": old.name().as_str(), "comment": old.comment()},
            "new": {"name": new.name().as_str(), "comment": new.comment()}
        }),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "ip_address",
        Some(assignment.id()),
        assignment.address().as_str(),
        "create",
        json!({"host_id": assignment.host_id(), "address": assignment.address().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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
    // Fetch all IP assignments to find the current one for old-value capture.
    let all = store.list_ip_addresses(&PageRequest::all()).await?;
    let old: &IpAddressAssignment = all
        .items
        .iter()
        .find(|a| a.address().as_str() == address.as_str())
        .ok_or_else(|| AppError::not_found(format!("IP address {}", address.as_str())))?;
    let old_mac: Option<String> = old.mac_address().map(|m| m.as_str());

    let updated = store.update_ip_address(address, command).await?;
    let new_mac = updated.mac_address().map(|m| m.as_str());

    let audit_event = CreateHistoryEvent::new(
        "system",
        "ip_address",
        Some(updated.id()),
        updated.address().as_str(),
        "update",
        json!({
            "old": {"mac_address": old_mac},
            "new": {"mac_address": new_mac}
        }),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "ip_address",
        Some(old.id()),
        old.address().as_str(),
        "delete",
        json!({"address": old.address().as_str(), "host_id": old.host_id()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(())
}
