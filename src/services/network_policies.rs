use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{CreateNetworkPolicy, NetworkPolicy},
        pagination::{Page, PageRequest},
        types::NetworkPolicyName,
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
                actor::SYSTEM,
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
                actor::SYSTEM,
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
