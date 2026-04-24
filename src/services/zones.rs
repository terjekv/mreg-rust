use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        pagination::{Page, PageRequest},
        types::ZoneName,
        zone::{
            CreateForwardZone, CreateForwardZoneDelegation, CreateReverseZone,
            CreateReverseZoneDelegation, ForwardZone, ForwardZoneDelegation, ReverseZone,
            ReverseZoneDelegation, UpdateForwardZone, UpdateReverseZone,
        },
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, ZoneStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "forward_zone"))]
pub async fn list_forward(
    store: &(dyn ZoneStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ForwardZone>, AppError> {
    store.list_forward_zones(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "forward_zone"))]
pub async fn create_forward(
    storage: &DynStorage,
    command: CreateForwardZone,
    events: &EventSinkClient,
) -> Result<ForwardZone, AppError> {
    let (zone, history) = storage
        .transaction(move |tx| {
            let zone = tx.zones().create_forward_zone(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "forward_zone",
                Some(zone.id()),
                zone.name().as_str(),
                actions::CREATE,
                json!({"name": zone.name().as_str()}),
            ))?;
            Ok((zone, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(zone)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "forward_zone"))]
pub async fn get_forward(
    store: &(dyn ZoneStore + Send + Sync),
    name: &ZoneName,
) -> Result<ForwardZone, AppError> {
    store.get_forward_zone_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "forward_zone"))]
pub async fn update_forward(
    storage: &DynStorage,
    name: &ZoneName,
    command: UpdateForwardZone,
    events: &EventSinkClient,
) -> Result<ForwardZone, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.zones().get_forward_zone_by_name(&name_owned)?;
            let new = tx.zones().update_forward_zone(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "forward_zone",
                Some(new.id()),
                new.name().as_str(),
                actions::UPDATE,
                json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "forward_zone"))]
pub async fn delete_forward(
    storage: &DynStorage,
    name: &ZoneName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.zones().get_forward_zone_by_name(&name_owned)?;
            tx.zones().delete_forward_zone(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "forward_zone",
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

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "reverse_zone"))]
pub async fn list_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ReverseZone>, AppError> {
    store.list_reverse_zones(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "reverse_zone"))]
pub async fn create_reverse(
    storage: &DynStorage,
    command: CreateReverseZone,
    events: &EventSinkClient,
) -> Result<ReverseZone, AppError> {
    let (zone, history) = storage
        .transaction(move |tx| {
            let zone = tx.zones().create_reverse_zone(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "reverse_zone",
                Some(zone.id()),
                zone.name().as_str(),
                actions::CREATE,
                json!({"name": zone.name().as_str()}),
            ))?;
            Ok((zone, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(zone)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "reverse_zone"))]
pub async fn get_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    name: &ZoneName,
) -> Result<ReverseZone, AppError> {
    store.get_reverse_zone_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "reverse_zone"))]
pub async fn update_reverse(
    storage: &DynStorage,
    name: &ZoneName,
    command: UpdateReverseZone,
    events: &EventSinkClient,
) -> Result<ReverseZone, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.zones().get_reverse_zone_by_name(&name_owned)?;
            let new = tx.zones().update_reverse_zone(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "reverse_zone",
                Some(new.id()),
                new.name().as_str(),
                actions::UPDATE,
                json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "reverse_zone"))]
pub async fn delete_reverse(
    storage: &DynStorage,
    name: &ZoneName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.zones().get_reverse_zone_by_name(&name_owned)?;
            tx.zones().delete_reverse_zone(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "reverse_zone",
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

// --- Forward zone delegation service functions ---

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "forward_zone_delegation")
)]
pub async fn list_forward_delegations(
    store: &(dyn ZoneStore + Send + Sync),
    zone_name: &ZoneName,
    page: &PageRequest,
) -> Result<Page<ForwardZoneDelegation>, AppError> {
    store.list_forward_zone_delegations(zone_name, page).await
}

#[tracing::instrument(
    skip(storage, events),
    fields(resource_kind = "forward_zone_delegation")
)]
pub async fn create_forward_delegation(
    storage: &DynStorage,
    command: CreateForwardZoneDelegation,
    events: &EventSinkClient,
) -> Result<ForwardZoneDelegation, AppError> {
    let (delegation, history) = storage
        .transaction(move |tx| {
            let delegation = tx.zones().create_forward_zone_delegation(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "forward_zone_delegation",
                Some(delegation.id()),
                delegation.name().as_str(),
                actions::CREATE,
                json!({"name": delegation.name().as_str()}),
            ))?;
            Ok((delegation, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(delegation)
}

#[tracing::instrument(
    skip(storage, events),
    fields(resource_kind = "forward_zone_delegation")
)]
pub async fn delete_forward_delegation(
    storage: &DynStorage,
    delegation_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            tx.zones().delete_forward_zone_delegation(delegation_id)?;
            let id_str = delegation_id.to_string();
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "forward_zone_delegation",
                Some(delegation_id),
                id_str.clone(),
                actions::DELETE,
                json!({"id": id_str}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

// --- Reverse zone delegation service functions ---

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "reverse_zone_delegation")
)]
pub async fn list_reverse_delegations(
    store: &(dyn ZoneStore + Send + Sync),
    zone_name: &ZoneName,
    page: &PageRequest,
) -> Result<Page<ReverseZoneDelegation>, AppError> {
    store.list_reverse_zone_delegations(zone_name, page).await
}

#[tracing::instrument(
    skip(storage, events),
    fields(resource_kind = "reverse_zone_delegation")
)]
pub async fn create_reverse_delegation(
    storage: &DynStorage,
    command: CreateReverseZoneDelegation,
    events: &EventSinkClient,
) -> Result<ReverseZoneDelegation, AppError> {
    let (delegation, history) = storage
        .transaction(move |tx| {
            let delegation = tx.zones().create_reverse_zone_delegation(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "reverse_zone_delegation",
                Some(delegation.id()),
                delegation.name().as_str(),
                actions::CREATE,
                json!({"name": delegation.name().as_str()}),
            ))?;
            Ok((delegation, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(delegation)
}

#[tracing::instrument(
    skip(storage, events),
    fields(resource_kind = "reverse_zone_delegation")
)]
pub async fn delete_reverse_delegation(
    storage: &DynStorage,
    delegation_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            tx.zones().delete_reverse_zone_delegation(delegation_id)?;
            let id_str = delegation_id.to_string();
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "reverse_zone_delegation",
                Some(delegation_id),
                id_str.clone(),
                actions::DELETE,
                json!({"id": id_str}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
