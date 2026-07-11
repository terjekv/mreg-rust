use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{
            CreateNetworkPolicy, CreateNetworkPolicyAttribute, NetworkPolicy,
            NetworkPolicyAttribute, NetworkPolicyDetails, UpdateNetworkPolicy,
            UpdateNetworkPolicyAttribute,
        },
        pagination::{Page, PageRequest},
        types::{NetworkPolicyAttributeName, NetworkPolicyName},
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, NetworkPolicyStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network_policy"))]
pub async fn list_network_policies(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    page: &PageRequest,
    filter: &NetworkPolicyFilter,
) -> Result<Page<NetworkPolicy>, AppError> {
    store.list_network_policies(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "network_policy"))]
pub async fn create_network_policy(
    storage: &DynStorage,
    command: CreateNetworkPolicy,
    events: &EventSinkClient,
) -> Result<NetworkPolicy, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            let item = tx.network_policies().create_network_policy(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "network_policy",
                Some(item.id()),
                item.name().as_str(),
                actions::CREATE,
                json!({"name": item.name().as_str(), "description": item.description()}),
            ))?;
            Ok((item, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network_policy"))]
pub async fn get_network_policy(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    name: &NetworkPolicyName,
) -> Result<NetworkPolicy, AppError> {
    store.get_network_policy_by_name(name).await
}

pub async fn get_network_policy_details(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    name: &NetworkPolicyName,
) -> Result<NetworkPolicyDetails, AppError> {
    let policy = store.get_network_policy_by_name(name).await?;
    let attributes = store.list_network_policy_attribute_values(name).await?;
    Ok(NetworkPolicyDetails::new(policy, attributes))
}

pub async fn update_network_policy(
    storage: &DynStorage,
    name: &NetworkPolicyName,
    command: UpdateNetworkPolicy,
    events: &EventSinkClient,
) -> Result<NetworkPolicy, AppError> {
    let name = name.clone();
    let (item, history) = storage.transaction(move |tx| {
        let old = tx.network_policies().get_network_policy_by_name(&name)?;
        let item = tx.network_policies().update_network_policy(&name, command)?;
        let event = tx.audit().record_event(CreateHistoryEvent::new(
            actor::current(), "network_policy", Some(item.id()), item.name().as_str(),
            actions::UPDATE,
            json!({"old_name": old.name().as_str(), "name": item.name().as_str(), "description": item.description()}),
        ))?;
        Ok((item, event))
    }).await?;
    events.emit(&DomainEvent::from(&history)).await;
    Ok(item)
}

pub async fn list_network_policy_attributes(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    page: &PageRequest,
) -> Result<Page<NetworkPolicyAttribute>, AppError> {
    store.list_network_policy_attributes(page).await
}

pub async fn create_network_policy_attribute(
    storage: &DynStorage,
    command: CreateNetworkPolicyAttribute,
    events: &EventSinkClient,
) -> Result<NetworkPolicyAttribute, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            let item = tx
                .network_policies()
                .create_network_policy_attribute(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "network_policy_attribute",
                Some(item.id()),
                item.name().as_str(),
                actions::CREATE,
                json!({"name": item.name().as_str(), "description": item.description()}),
            ))?;
            Ok((item, event))
        })
        .await?;
    events.emit(&DomainEvent::from(&history)).await;
    Ok(item)
}

pub async fn get_network_policy_attribute(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    name: &NetworkPolicyAttributeName,
) -> Result<NetworkPolicyAttribute, AppError> {
    store.get_network_policy_attribute_by_name(name).await
}

pub async fn update_network_policy_attribute(
    storage: &DynStorage,
    name: &NetworkPolicyAttributeName,
    command: UpdateNetworkPolicyAttribute,
    events: &EventSinkClient,
) -> Result<NetworkPolicyAttribute, AppError> {
    if is_protected_policy_attribute(name.as_str())
        && command
            .name
            .as_ref()
            .is_some_and(|new_name| new_name != name)
    {
        return Err(AppError::forbidden(format!(
            "Cannot rename protected attribute '{}'.",
            name
        )));
    }
    let name = name.clone();
    let (item, history) = storage.transaction(move |tx| {
        let old = tx.network_policies().get_network_policy_attribute_by_name(&name)?;
        let item = tx.network_policies().update_network_policy_attribute(&name, command)?;
        let event = tx.audit().record_event(CreateHistoryEvent::new(
            actor::current(), "network_policy_attribute", Some(item.id()), item.name().as_str(),
            actions::UPDATE, json!({"old_name": old.name().as_str(), "name": item.name().as_str(), "description": item.description()}),
        ))?;
        Ok((item, event))
    }).await?;
    events.emit(&DomainEvent::from(&history)).await;
    Ok(item)
}

pub async fn delete_network_policy_attribute(
    storage: &DynStorage,
    name: &NetworkPolicyAttributeName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    if is_protected_policy_attribute(name.as_str()) {
        return Err(AppError::forbidden(format!(
            "Cannot delete the attribute '{}', it is protected.",
            name
        )));
    }
    let name = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx
                .network_policies()
                .get_network_policy_attribute_by_name(&name)?;
            tx.network_policies()
                .delete_network_policy_attribute(&name)?;
            tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "network_policy_attribute",
                Some(old.id()),
                old.name().as_str(),
                actions::DELETE,
                json!({"name": old.name().as_str(), "description": old.description()}),
            ))
        })
        .await?;
    events.emit(&DomainEvent::from(&history)).await;
    Ok(())
}

fn is_protected_policy_attribute(name: &str) -> bool {
    name == "isolated"
        || std::env::var("MREG_PROTECTED_POLICY_ATTRIBUTES")
            .ok()
            .is_some_and(|value| value.split(',').map(str::trim).any(|item| item == name))
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "network_policy"))]
pub async fn delete_network_policy(
    storage: &DynStorage,
    name: &NetworkPolicyName,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let name_owned = name.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx
                .network_policies()
                .get_network_policy_by_name(&name_owned)?;
            tx.network_policies().delete_network_policy(&name_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "network_policy",
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
