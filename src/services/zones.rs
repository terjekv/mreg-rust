use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::actions,
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
    events::EventSinkClient,
    storage::{AuditStore, ZoneStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "forward_zone"))]
pub async fn list_forward(
    store: &(dyn ZoneStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ForwardZone>, AppError> {
    store.list_forward_zones(page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "forward_zone"))]
pub async fn create_forward(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateForwardZone,
) -> Result<ForwardZone, AppError> {
    let zone = store.create_forward_zone(command).await?;

    super::audit_mutation(
        audit,
        events,
        "forward_zone",
        actions::CREATE,
        Some(zone.id()),
        zone.name().as_str(),
        json!({"name": zone.name().as_str()}),
    )
    .await;

    Ok(zone)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "forward_zone"))]
pub async fn get_forward(
    store: &(dyn ZoneStore + Send + Sync),
    name: &ZoneName,
) -> Result<ForwardZone, AppError> {
    store.get_forward_zone_by_name(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "forward_zone"))]
pub async fn update_forward(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &ZoneName,
    command: UpdateForwardZone,
) -> Result<ForwardZone, AppError> {
    let old = store.get_forward_zone_by_name(name).await?;
    let new = store.update_forward_zone(name, command).await?;

    super::audit_mutation(
        audit,
        events,
        "forward_zone",
        actions::UPDATE,
        Some(new.id()),
        new.name().as_str(),
        json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "forward_zone"))]
pub async fn delete_forward(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &ZoneName,
) -> Result<(), AppError> {
    let old = store.get_forward_zone_by_name(name).await?;
    store.delete_forward_zone(name).await?;

    super::audit_mutation(
        audit,
        events,
        "forward_zone",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str()}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "reverse_zone"))]
pub async fn list_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<ReverseZone>, AppError> {
    store.list_reverse_zones(page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "reverse_zone"))]
pub async fn create_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateReverseZone,
) -> Result<ReverseZone, AppError> {
    let zone = store.create_reverse_zone(command).await?;

    super::audit_mutation(
        audit,
        events,
        "reverse_zone",
        actions::CREATE,
        Some(zone.id()),
        zone.name().as_str(),
        json!({"name": zone.name().as_str()}),
    )
    .await;

    Ok(zone)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "reverse_zone"))]
pub async fn get_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    name: &ZoneName,
) -> Result<ReverseZone, AppError> {
    store.get_reverse_zone_by_name(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "reverse_zone"))]
pub async fn update_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &ZoneName,
    command: UpdateReverseZone,
) -> Result<ReverseZone, AppError> {
    let old = store.get_reverse_zone_by_name(name).await?;
    let new = store.update_reverse_zone(name, command).await?;

    super::audit_mutation(
        audit,
        events,
        "reverse_zone",
        actions::UPDATE,
        Some(new.id()),
        new.name().as_str(),
        json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "reverse_zone"))]
pub async fn delete_reverse(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &ZoneName,
) -> Result<(), AppError> {
    let old = store.get_reverse_zone_by_name(name).await?;
    store.delete_reverse_zone(name).await?;

    super::audit_mutation(
        audit,
        events,
        "reverse_zone",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str()}),
    )
    .await;

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
    skip(store, audit, events),
    fields(resource_kind = "forward_zone_delegation")
)]
pub async fn create_forward_delegation(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateForwardZoneDelegation,
) -> Result<ForwardZoneDelegation, AppError> {
    let delegation = store.create_forward_zone_delegation(command).await?;

    super::audit_mutation(
        audit,
        events,
        "forward_zone_delegation",
        actions::CREATE,
        Some(delegation.id()),
        delegation.name().as_str(),
        json!({"name": delegation.name().as_str()}),
    )
    .await;

    Ok(delegation)
}

#[tracing::instrument(
    skip(store, audit, events),
    fields(resource_kind = "forward_zone_delegation")
)]
pub async fn delete_forward_delegation(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    delegation_id: Uuid,
) -> Result<(), AppError> {
    store.delete_forward_zone_delegation(delegation_id).await?;

    let id_str = delegation_id.to_string();
    super::audit_mutation(
        audit,
        events,
        "forward_zone_delegation",
        actions::DELETE,
        Some(delegation_id),
        &id_str,
        json!({"id": id_str}),
    )
    .await;

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
    skip(store, audit, events),
    fields(resource_kind = "reverse_zone_delegation")
)]
pub async fn create_reverse_delegation(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateReverseZoneDelegation,
) -> Result<ReverseZoneDelegation, AppError> {
    let delegation = store.create_reverse_zone_delegation(command).await?;

    super::audit_mutation(
        audit,
        events,
        "reverse_zone_delegation",
        actions::CREATE,
        Some(delegation.id()),
        delegation.name().as_str(),
        json!({"name": delegation.name().as_str()}),
    )
    .await;

    Ok(delegation)
}

#[tracing::instrument(
    skip(store, audit, events),
    fields(resource_kind = "reverse_zone_delegation")
)]
pub async fn delete_reverse_delegation(
    store: &(dyn ZoneStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    delegation_id: Uuid,
) -> Result<(), AppError> {
    store.delete_reverse_zone_delegation(delegation_id).await?;

    let id_str = delegation_id.to_string();
    super::audit_mutation(
        audit,
        events,
        "reverse_zone_delegation",
        actions::DELETE,
        Some(delegation_id),
        &id_str,
        json!({"id": id_str}),
    )
    .await;

    Ok(())
}
