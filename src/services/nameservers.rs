use serde_json::json;

use crate::{
    audit::CreateHistoryEvent,
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{Page, PageRequest},
        types::DnsName,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, NameServerStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "nameserver"))]
pub async fn list(
    store: &(dyn NameServerStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<NameServer>, AppError> {
    store.list_nameservers(page).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "nameserver"))]
pub async fn create(
    store: &(dyn NameServerStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateNameServer,
) -> Result<NameServer, AppError> {
    let ns = store.create_nameserver(command).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "nameserver",
        Some(ns.id()),
        ns.name().as_str(),
        "create",
        json!({"name": ns.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(ns)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "nameserver"))]
pub async fn get(
    store: &(dyn NameServerStore + Send + Sync),
    name: &DnsName,
) -> Result<NameServer, AppError> {
    store.get_nameserver_by_name(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "nameserver"))]
pub async fn update(
    store: &(dyn NameServerStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &DnsName,
    command: UpdateNameServer,
) -> Result<NameServer, AppError> {
    let old = store.get_nameserver_by_name(name).await?;
    let new = store.update_nameserver(name, command).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "nameserver",
        Some(new.id()),
        new.name().as_str(),
        "update",
        json!({"old": {"name": old.name().as_str()}, "new": {"name": new.name().as_str()}}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "nameserver"))]
pub async fn delete(
    store: &(dyn NameServerStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &DnsName,
) -> Result<(), AppError> {
    let old = store.get_nameserver_by_name(name).await?;
    store.delete_nameserver(name).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "nameserver",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(())
}
