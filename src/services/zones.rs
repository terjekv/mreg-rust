use serde_json::json;

use crate::{
    audit::CreateHistoryEvent,
    domain::{
        pagination::{Page, PageRequest},
        types::ZoneName,
        zone::{
            CreateForwardZone, CreateReverseZone, ForwardZone, ReverseZone, UpdateForwardZone,
            UpdateReverseZone,
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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "forward_zone",
        Some(zone.id()),
        zone.name().as_str(),
        "create",
        json!({"name": zone.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "forward_zone",
        Some(new.id()),
        new.name().as_str(),
        "update",
        json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "forward_zone",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "reverse_zone",
        Some(zone.id()),
        zone.name().as_str(),
        "create",
        json!({"name": zone.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "reverse_zone",
        Some(new.id()),
        new.name().as_str(),
        "update",
        json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "reverse_zone",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(())
}
