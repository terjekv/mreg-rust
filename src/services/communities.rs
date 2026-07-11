use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        community::{Community, CreateCommunity, UpdateCommunity},
        filters::CommunityFilter,
        pagination::{Page, PageRequest},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{CommunityStore, DynStorage},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "community"))]
pub async fn list_communities(
    store: &(dyn CommunityStore + Send + Sync),
    page: &PageRequest,
    filter: &CommunityFilter,
) -> Result<Page<Community>, AppError> {
    store.list_communities(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "community"))]
pub async fn create_community(
    storage: &DynStorage,
    command: CreateCommunity,
    events: &EventSinkClient,
) -> Result<Community, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            let network = tx
                .networks()
                .get_network_by_cidr(command.network_cidr())?;
            let policy = tx
                .network_policies()
                .get_network_policy_by_name(command.policy_name())?;
            if network.policy_id() != Some(policy.id()) {
                return Err(AppError::not_acceptable(format!(
                    "network '{}' is not assigned policy '{}'",
                    network.cidr().as_str(),
                    policy.name().as_str()
                )));
            }
            if let Some(limit) = network.max_communities() {
                let count = tx
                    .communities()
                    .list_communities(&PageRequest::all(), &CommunityFilter::default())?
                    .items
                    .into_iter()
                    .filter(|community| community.network_cidr() == network.cidr())
                    .count();
                if count >= limit.as_u32() as usize {
                    return Err(AppError::not_acceptable(format!(
                        "network '{}' already has the maximum allowed communities ({})",
                        network.cidr().as_str(),
                        limit.as_u32()
                    )));
                }
            }
            let item = tx.communities().create_community(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "community",
                Some(item.id()),
                item.name().as_str(),
                actions::CREATE,
                json!({"name": item.name().as_str(), "policy_name": item.policy_name().as_str(), "description": item.description()}),
            ))?;
            Ok((item, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(item)
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "community"))]
pub async fn update_community(
    storage: &DynStorage,
    community_id: Uuid,
    command: UpdateCommunity,
    events: &EventSinkClient,
) -> Result<Community, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            let old = tx.communities().get_community(community_id)?;
            let item = tx.communities().update_community(community_id, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "community",
                Some(item.id()),
                item.name().as_str(),
                actions::UPDATE,
                json!({
                    "old": {"name": old.name().as_str(), "description": old.description()},
                    "new": {"name": item.name().as_str(), "description": item.description()},
                }),
            ))?;
            Ok((item, event))
        })
        .await?;
    events.emit(&DomainEvent::from(&history)).await;
    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "community"))]
pub async fn get_community(
    store: &(dyn CommunityStore + Send + Sync),
    community_id: Uuid,
) -> Result<Community, AppError> {
    store.get_community(community_id).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "community"))]
pub async fn delete_community(
    storage: &DynStorage,
    community_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx.communities().get_community(community_id)?;
            tx.communities().delete_community(community_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "community",
                Some(old.id()),
                old.name().as_str(),
                actions::DELETE,
                json!({"name": old.name().as_str(), "policy_name": old.policy_name().as_str(), "description": old.description()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "community"))]
pub async fn find_community_by_names(
    store: &(dyn CommunityStore + Send + Sync),
    policy_name: &NetworkPolicyName,
    community_name: &CommunityName,
) -> Result<Community, AppError> {
    store
        .find_community_by_names(policy_name, community_name)
        .await
}
