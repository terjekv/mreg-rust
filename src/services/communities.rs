use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        community::{Community, CreateCommunity},
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
            let item = tx.communities().create_community(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
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
                actor::SYSTEM,
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
