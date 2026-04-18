use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::actions,
    domain::{
        community::{Community, CreateCommunity},
        filters::CommunityFilter,
        pagination::{Page, PageRequest},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, CommunityStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "community"))]
pub async fn list_communities(
    store: &(dyn CommunityStore + Send + Sync),
    page: &PageRequest,
    filter: &CommunityFilter,
) -> Result<Page<Community>, AppError> {
    store.list_communities(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "community"))]
pub async fn create_community(
    store: &(dyn CommunityStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateCommunity,
) -> Result<Community, AppError> {
    let item = store.create_community(command).await?;

    super::audit_mutation(
        audit,
        events,
        "community",
        actions::CREATE,
        Some(item.id()),
        item.name().as_str(),
        json!({"name": item.name().as_str(), "policy_name": item.policy_name().as_str(), "description": item.description()}),
    )
    .await;

    Ok(item)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "community"))]
pub async fn get_community(
    store: &(dyn CommunityStore + Send + Sync),
    community_id: Uuid,
) -> Result<Community, AppError> {
    store.get_community(community_id).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "community"))]
pub async fn delete_community(
    store: &(dyn CommunityStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    community_id: Uuid,
) -> Result<(), AppError> {
    let old = store.get_community(community_id).await?;
    store.delete_community(community_id).await?;

    super::audit_mutation(
        audit,
        events,
        "community",
        actions::DELETE,
        Some(old.id()),
        old.name().as_str(),
        json!({"name": old.name().as_str(), "policy_name": old.policy_name().as_str(), "description": old.description()}),
    )
    .await;

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
