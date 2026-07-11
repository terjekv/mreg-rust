use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
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
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, HostStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host"))]
pub async fn list(
    store: &(dyn HostStore + Send + Sync),
    page: &PageRequest,
    filter: &HostFilter,
) -> Result<Page<Host>, AppError> {
    store.list_hosts(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host"))]
pub async fn create(
    storage: &DynStorage,
    command: CreateHost,
    events: &EventSinkClient,
) -> Result<Host, AppError> {
    let (host, history) = storage
        .transaction(move |tx| {
            let host = tx.hosts().create_host(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host",
                Some(host.id()),
                host.name().as_str(),
                actions::CREATE,
                json!({"name": host.name().as_str(), "comment": host.comment()}),
            ))?;
            Ok((host, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

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

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host"))]
pub async fn update(
    storage: &DynStorage,
    name: &Hostname,
    command: UpdateHost,
    events: &EventSinkClient,
) -> Result<Host, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.hosts().get_host_by_name(&name_owned)?;
            let new = tx.hosts().update_host(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host",
                Some(new.id()),
                new.name().as_str(),
                actions::UPDATE,
                json!({
                    "old": {"name": old.name().as_str(), "comment": old.comment()},
                    "new": {"name": new.name().as_str(), "comment": new.comment()}
                }),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host"))]
pub async fn delete(
    storage: &DynStorage,
    name: &Hostname,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.hosts().get_host_by_name(&name_owned)?;
            tx.hosts().delete_host(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host",
                Some(old.id()),
                old.name().as_str(),
                actions::DELETE,
                json!({"name": old.name().as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

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

#[tracing::instrument(skip(storage, events), fields(resource_kind = "ip_address"))]
pub async fn assign_ip_address(
    storage: &DynStorage,
    command: AssignIpAddress,
    events: &EventSinkClient,
) -> Result<IpAddressAssignment, AppError> {
    let (assignment, history) = storage
        .transaction(move |tx| {
            let assignment = tx.hosts().assign_ip_address(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "ip_address",
                Some(assignment.id()),
                assignment.address().as_str(),
                actions::CREATE,
                json!({"host_id": assignment.host_id(), "address": assignment.address().as_str()}),
            ))?;
            Ok((assignment, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(assignment)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "ip_address"))]
pub async fn update_ip_address(
    storage: &DynStorage,
    address: &IpAddressValue,
    command: UpdateIpAddress,
    events: &EventSinkClient,
) -> Result<IpAddressAssignment, AppError> {
    let address_owned = *address;
    let (updated, history) = storage
        .transaction(move |tx| {
            let old = tx.hosts().get_ip_address(&address_owned)?;
            let old_mac: Option<String> = old.mac_address().map(|m| m.as_str());
            let updated = tx.hosts().update_ip_address(&address_owned, command)?;
            let new_mac = updated.mac_address().map(|m| m.as_str());
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "ip_address",
                Some(updated.id()),
                updated.address().as_str(),
                actions::UPDATE,
                json!({
                    "old": {"mac_address": old_mac},
                    "new": {"mac_address": new_mac}
                }),
            ))?;
            Ok((updated, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(updated)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "ip_address"))]
pub async fn unassign_ip_address(
    storage: &DynStorage,
    address: &IpAddressValue,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let address_owned = *address;
    let history = storage
        .transaction(move |tx| {
            let old = tx.hosts().unassign_ip_address(&address_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "ip_address",
                Some(old.id()),
                old.address().as_str(),
                actions::DELETE,
                json!({"address": old.address().as_str(), "host_id": old.host_id()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
