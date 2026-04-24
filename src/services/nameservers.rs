use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{Page, PageRequest},
        types::DnsName,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, NameServerStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "nameserver"))]
pub async fn list(
    store: &(dyn NameServerStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<NameServer>, AppError> {
    store.list_nameservers(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "nameserver"))]
pub async fn create(
    storage: &DynStorage,
    command: CreateNameServer,
    events: &EventSinkClient,
) -> Result<NameServer, AppError> {
    let (ns, history) = storage
        .transaction(move |tx| {
            let ns = tx.nameservers().create_nameserver(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "nameserver",
                Some(ns.id()),
                ns.name().as_str(),
                actions::CREATE,
                json!({"name": ns.name().as_str()}),
            ))?;
            Ok((ns, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(ns)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "nameserver"))]
pub async fn get(
    store: &(dyn NameServerStore + Send + Sync),
    name: &DnsName,
) -> Result<NameServer, AppError> {
    store.get_nameserver_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "nameserver"))]
pub async fn update(
    storage: &DynStorage,
    name: &DnsName,
    command: UpdateNameServer,
    events: &EventSinkClient,
) -> Result<NameServer, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.nameservers().get_nameserver_by_name(&name_owned)?;
            let new = tx.nameservers().update_nameserver(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "nameserver",
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

#[tracing::instrument(skip(storage, events), fields(resource_kind = "nameserver"))]
pub async fn delete(
    storage: &DynStorage,
    name: &DnsName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.nameservers().get_nameserver_by_name(&name_owned)?;
            tx.nameservers().delete_nameserver(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "nameserver",
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
