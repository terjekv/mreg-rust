use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        host_policy::{
            CreateHostPolicyAtom, CreateHostPolicyRole, HostPolicyAtom, HostPolicyRole,
            UpdateHostPolicyAtom, UpdateHostPolicyRole,
        },
        pagination::{Page, PageRequest},
        types::HostPolicyName,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, HostPolicyStore},
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

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_atom"))]
pub async fn create_atom(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateHostPolicyAtom,
) -> Result<HostPolicyAtom, AppError> {
    let atom = store.create_atom(command).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_atom",
        actions::CREATE,
        Some(atom.id()),
        atom.name().as_str(),
        json!({"name": atom.name().as_str(), "description": atom.description()}),
    )
    .await;

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

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_atom"))]
pub async fn update_atom(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &HostPolicyName,
    command: UpdateHostPolicyAtom,
) -> Result<HostPolicyAtom, AppError> {
    let old = store.get_atom_by_name(name).await?;
    let new = store.update_atom(name, command).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_atom",
        actions::UPDATE,
        Some(new.id()),
        new.name().as_str(),
        json!({"old": {"description": old.description()}, "new": {"description": new.description()}}),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_atom"))]
pub async fn delete_atom(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &HostPolicyName,
) -> Result<(), AppError> {
    let old = store.get_atom_by_name(name).await?;
    store.delete_atom(name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_atom",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str(), "description": old.description()}),
    )
    .await;

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

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn create_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateHostPolicyRole,
) -> Result<HostPolicyRole, AppError> {
    let role = store.create_role(command).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::CREATE,
        Some(role.id()),
        role.name().as_str(),
        json!({"name": role.name().as_str(), "description": role.description()}),
    )
    .await;

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

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn update_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &HostPolicyName,
    command: UpdateHostPolicyRole,
) -> Result<HostPolicyRole, AppError> {
    let old = store.get_role_by_name(name).await?;
    let new = store.update_role(name, command).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::UPDATE,
        Some(new.id()),
        new.name().as_str(),
        json!({"old": {"description": old.description()}, "new": {"description": new.description()}}),
    )
    .await;

    Ok(new)
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn delete_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &HostPolicyName,
) -> Result<(), AppError> {
    let old = store.get_role_by_name(name).await?;
    store.delete_role(name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str(), "description": old.description()}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn add_atom_to_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    role_name: &HostPolicyName,
    atom_name: &HostPolicyName,
) -> Result<(), AppError> {
    let role = store.get_role_by_name(role_name).await?;
    store.add_atom_to_role(role_name, atom_name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::ADD_ATOM,
        Some(role.id()),
        role.name().as_str(),
        json!({"role": role.name().as_str(), "atom": atom_name.as_str()}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn remove_atom_from_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    role_name: &HostPolicyName,
    atom_name: &HostPolicyName,
) -> Result<(), AppError> {
    let role = store.get_role_by_name(role_name).await?;
    store.remove_atom_from_role(role_name, atom_name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::REMOVE_ATOM,
        Some(role.id()),
        role.name().as_str(),
        json!({"role": role.name().as_str(), "atom": atom_name.as_str()}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn add_host_to_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    role_name: &HostPolicyName,
    host_name: &str,
) -> Result<(), AppError> {
    let role = store.get_role_by_name(role_name).await?;
    store.add_host_to_role(role_name, host_name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::ADD_HOST,
        Some(role.id()),
        role.name().as_str(),
        json!({"role": role.name().as_str(), "host": host_name}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn remove_host_from_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    role_name: &HostPolicyName,
    host_name: &str,
) -> Result<(), AppError> {
    let role = store.get_role_by_name(role_name).await?;
    store.remove_host_from_role(role_name, host_name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::REMOVE_HOST,
        Some(role.id()),
        role.name().as_str(),
        json!({"role": role.name().as_str(), "host": host_name}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn add_label_to_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    role_name: &HostPolicyName,
    label_name: &str,
) -> Result<(), AppError> {
    let role = store.get_role_by_name(role_name).await?;
    store.add_label_to_role(role_name, label_name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::ADD_LABEL,
        Some(role.id()),
        role.name().as_str(),
        json!({"role": role.name().as_str(), "label": label_name}),
    )
    .await;

    Ok(())
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_policy_role"))]
pub async fn remove_label_from_role(
    store: &(dyn HostPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    role_name: &HostPolicyName,
    label_name: &str,
) -> Result<(), AppError> {
    let role = store.get_role_by_name(role_name).await?;
    store.remove_label_from_role(role_name, label_name).await?;

    super::audit_mutation(
        audit,
        events,
        "host_policy_role",
        actions::REMOVE_LABEL,
        Some(role.id()),
        role.name().as_str(),
        json!({"role": role.name().as_str(), "label": label_name}),
    )
    .await;

    Ok(())
}
