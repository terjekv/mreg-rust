use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        host_policy::{
            CreateHostPolicyAtom, CreateHostPolicyRole, HostPolicyAtom, HostPolicyRole,
            UpdateHostPolicyAtom, UpdateHostPolicyRole,
        },
        pagination::{Page, PageRequest},
        types::HostPolicyName,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, HostPolicyStore},
};

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "host_policy_atom")
)]
pub async fn list_atoms(
    store: &(dyn HostPolicyStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<HostPolicyAtom>, AppError> {
    store.list_atoms(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_atom"))]
pub async fn create_atom(
    storage: &DynStorage,
    command: CreateHostPolicyAtom,
    events: &EventSinkClient,
) -> Result<HostPolicyAtom, AppError> {
    let (atom, history) = storage
        .transaction(move |tx| {
            let atom = tx.host_policy().create_atom(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_atom",
                Some(atom.id()),
                atom.name().as_str(),
                actions::CREATE,
                json!({"name": atom.name().as_str(), "description": atom.description()}),
            ))?;
            Ok((atom, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(atom)
}

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "host_policy_atom")
)]
pub async fn get_atom(
    store: &(dyn HostPolicyStore + Send + Sync),
    name: &HostPolicyName,
) -> Result<HostPolicyAtom, AppError> {
    store.get_atom_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_atom"))]
pub async fn update_atom(
    storage: &DynStorage,
    name: &HostPolicyName,
    command: UpdateHostPolicyAtom,
    events: &EventSinkClient,
) -> Result<HostPolicyAtom, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.host_policy().get_atom_by_name(&name_owned)?;
            let new = tx.host_policy().update_atom(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_atom",
                Some(new.id()),
                new.name().as_str(),
                actions::UPDATE,
                json!({"old": {"description": old.description()}, "new": {"description": new.description()}}),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_atom"))]
pub async fn delete_atom(
    storage: &DynStorage,
    name: &HostPolicyName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.host_policy().get_atom_by_name(&name_owned)?;
            tx.host_policy().delete_atom(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_atom",
                Some(old.id()),
                old.name().as_str(),
                actions::DELETE,
                json!({"name": old.name().as_str(), "description": old.description()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "host_policy_role")
)]
pub async fn list_roles(
    store: &(dyn HostPolicyStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<HostPolicyRole>, AppError> {
    store.list_roles(page).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn create_role(
    storage: &DynStorage,
    command: CreateHostPolicyRole,
    events: &EventSinkClient,
) -> Result<HostPolicyRole, AppError> {
    let (role, history) = storage
        .transaction(move |tx| {
            let role = tx.host_policy().create_role(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::CREATE,
                json!({"name": role.name().as_str(), "description": role.description()}),
            ))?;
            Ok((role, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(role)
}

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "host_policy_role")
)]
pub async fn get_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    name: &HostPolicyName,
) -> Result<HostPolicyRole, AppError> {
    store.get_role_by_name(name).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn update_role(
    storage: &DynStorage,
    name: &HostPolicyName,
    command: UpdateHostPolicyRole,
    events: &EventSinkClient,
) -> Result<HostPolicyRole, AppError> {
    let name_owned = name.clone();
    let (new, history) = storage
        .transaction(move |tx| {
            let old = tx.host_policy().get_role_by_name(&name_owned)?;
            let new = tx.host_policy().update_role(&name_owned, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(new.id()),
                new.name().as_str(),
                actions::UPDATE,
                json!({"old": {"description": old.description()}, "new": {"description": new.description()}}),
            ))?;
            Ok((new, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(new)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn delete_role(
    storage: &DynStorage,
    name: &HostPolicyName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.host_policy().get_role_by_name(&name_owned)?;
            tx.host_policy().delete_role(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(old.id()),
                old.name().as_str(),
                actions::DELETE,
                json!({"name": old.name().as_str(), "description": old.description()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn add_atom_to_role(
    storage: &DynStorage,
    role_name: &HostPolicyName,
    atom_name: &HostPolicyName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let role_name_owned = role_name.clone();
    let atom_name_owned = atom_name.clone();
    let history = storage
        .transaction(move |tx| {
            let role = tx.host_policy().get_role_by_name(&role_name_owned)?;
            tx.host_policy()
                .add_atom_to_role(&role_name_owned, &atom_name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::ADD_ATOM,
                json!({"role": role.name().as_str(), "atom": atom_name_owned.as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn remove_atom_from_role(
    storage: &DynStorage,
    role_name: &HostPolicyName,
    atom_name: &HostPolicyName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let role_name_owned = role_name.clone();
    let atom_name_owned = atom_name.clone();
    let history = storage
        .transaction(move |tx| {
            let role = tx.host_policy().get_role_by_name(&role_name_owned)?;
            tx.host_policy()
                .remove_atom_from_role(&role_name_owned, &atom_name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::REMOVE_ATOM,
                json!({"role": role.name().as_str(), "atom": atom_name_owned.as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn add_host_to_role(
    storage: &DynStorage,
    role_name: &HostPolicyName,
    host_name: &str,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let role_name_owned = role_name.clone();
    let host_name_owned = host_name.to_owned();
    let history = storage
        .transaction(move |tx| {
            let role = tx.host_policy().get_role_by_name(&role_name_owned)?;
            tx.host_policy()
                .add_host_to_role(&role_name_owned, &host_name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::ADD_HOST,
                json!({"role": role.name().as_str(), "host": host_name_owned}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn remove_host_from_role(
    storage: &DynStorage,
    role_name: &HostPolicyName,
    host_name: &str,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let role_name_owned = role_name.clone();
    let host_name_owned = host_name.to_owned();
    let history = storage
        .transaction(move |tx| {
            let role = tx.host_policy().get_role_by_name(&role_name_owned)?;
            tx.host_policy()
                .remove_host_from_role(&role_name_owned, &host_name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::REMOVE_HOST,
                json!({"role": role.name().as_str(), "host": host_name_owned}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn add_label_to_role(
    storage: &DynStorage,
    role_name: &HostPolicyName,
    label_name: &str,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let role_name_owned = role_name.clone();
    let label_name_owned = label_name.to_owned();
    let history = storage
        .transaction(move |tx| {
            let role = tx.host_policy().get_role_by_name(&role_name_owned)?;
            tx.host_policy()
                .add_label_to_role(&role_name_owned, &label_name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::ADD_LABEL,
                json!({"role": role.name().as_str(), "label": label_name_owned}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_policy_role"))]
pub async fn remove_label_from_role(
    storage: &DynStorage,
    role_name: &HostPolicyName,
    label_name: &str,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let role_name_owned = role_name.clone();
    let label_name_owned = label_name.to_owned();
    let history = storage
        .transaction(move |tx| {
            let role = tx.host_policy().get_role_by_name(&role_name_owned)?;
            tx.host_policy()
                .remove_label_from_role(&role_name_owned, &label_name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_policy_role",
                Some(role.id()),
                role.name().as_str(),
                actions::REMOVE_LABEL,
                json!({"role": role.name().as_str(), "label": label_name_owned}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
