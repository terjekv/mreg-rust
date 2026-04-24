use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::NetworkFilter,
        host::IpAddressAssignment,
        network::{CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork},
        pagination::{Page, PageRequest},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, NetworkStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network"))]
pub async fn list(
    store: &(dyn NetworkStore + Send + Sync),
    page: &PageRequest,
    filter: &NetworkFilter,
) -> Result<Page<Network>, AppError> {
    store.list_networks(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "network"))]
pub async fn create(
    storage: &DynStorage,
    command: CreateNetwork,
    events: &EventSinkClient,
) -> Result<Network, AppError> {
    let (network, history) = storage
        .transaction(move |tx| {
            let network = tx.networks().create_network(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "network",
                Some(network.id()),
                network.cidr().as_str(),
                actions::CREATE,
                json!({
                    "cidr": network.cidr().as_str(),
                    "description": network.description(),
                }),
            ))?;
            Ok((network, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(network)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network"))]
pub async fn get(
    store: &(dyn NetworkStore + Send + Sync),
    cidr: &CidrValue,
) -> Result<Network, AppError> {
    store.get_network_by_cidr(cidr).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "network"))]
pub async fn delete(
    storage: &DynStorage,
    cidr: &CidrValue,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let cidr_owned = cidr.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.networks().get_network_by_cidr(&cidr_owned)?;
            tx.networks().delete_network(&cidr_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "network",
                Some(old.id()),
                old.cidr().as_str(),
                actions::DELETE,
                json!({
                    "cidr": old.cidr().as_str(),
                    "description": old.description(),
                }),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

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

#[tracing::instrument(skip(storage, events), fields(resource_kind = "network"))]
pub async fn update(
    storage: &DynStorage,
    cidr: &CidrValue,
    command: UpdateNetwork,
    events: &EventSinkClient,
) -> Result<Network, AppError> {
    let cidr_owned = cidr.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.networks().get_network_by_cidr(&cidr_owned)?;
            let new = tx.networks().update_network(&cidr_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "network",
                Some(new.id()),
                new.cidr().as_str(),
                actions::UPDATE,
                json!({
                    "old": {"description": old.description(), "vlan": old.vlan(), "frozen": old.frozen(), "reserved": old.reserved()},
                    "new": {"description": new.description(), "vlan": new.vlan(), "frozen": new.frozen(), "reserved": new.reserved()},
                }),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "excluded_range"))]
pub async fn add_excluded_range(
    storage: &DynStorage,
    cidr: &CidrValue,
    command: CreateExcludedRange,
    events: &EventSinkClient,
) -> Result<ExcludedRange, AppError> {
    let cidr_owned = cidr.clone();
    let (range, history) = storage
        .transaction(move |tx| {
            let range = tx.networks().add_excluded_range(&cidr_owned, command)?;
            let range_name = format!("{}-{}", range.start_ip().as_str(), range.end_ip().as_str());
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "excluded_range",
                Some(range.id()),
                range_name,
                actions::CREATE,
                json!({
                    "start_ip": range.start_ip().as_str(),
                    "end_ip": range.end_ip().as_str(),
                    "description": range.description(),
                }),
            ))?;
            Ok((range, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

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
