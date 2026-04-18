use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{CreateNetworkPolicy, NetworkPolicy},
        pagination::{Page, PageRequest},
        types::NetworkPolicyName,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, NetworkPolicyStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network_policy"))]
pub async fn list_network_policies(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    page: &PageRequest,
    filter: &NetworkPolicyFilter,
) -> Result<Page<NetworkPolicy>, AppError> {
    store.list_network_policies(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "network_policy"))]
pub async fn create_network_policy(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateNetworkPolicy,
) -> Result<NetworkPolicy, AppError> {
    let item = store.create_network_policy(command).await?;

    super::audit_mutation(
        audit,
        events,
        "network_policy",
        actions::CREATE,
        Some(item.id()),
        item.name().as_str(),
        json!({"name": item.name().as_str(), "description": item.description()}),
    )
    .await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "network_policy"))]
pub async fn get_network_policy(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    name: &NetworkPolicyName,
) -> Result<NetworkPolicy, AppError> {
    store.get_network_policy_by_name(name).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "network_policy"))]
pub async fn delete_network_policy(
    store: &(dyn NetworkPolicyStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    name: &NetworkPolicyName,
) -> Result<(), AppError> {
    let old = store.get_network_policy_by_name(name).await?;
    store.delete_network_policy(name).await?;

    super::audit_mutation(
        audit,
        events,
        "network_policy",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str(), "description": old.description()}),
    )
    .await;

    Ok(())
}
