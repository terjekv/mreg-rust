use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::CreateHistoryEvent,
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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "community",
        Some(item.id()),
        item.name().as_str(),
        "create",
        json!({"name": item.name().as_str(), "policy_name": item.policy_name().as_str(), "description": item.description()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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

    let audit_event = CreateHistoryEvent::new(
        "system",
        "community",
        Some(old.id()),
        old.name().as_str(),
        "delete",
        json!({"name": old.name().as_str(), "policy_name": old.policy_name().as_str(), "description": old.description()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

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
