use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::CreateHistoryEvent,
    domain::{
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, HostCommunityAssignmentStore},
};

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "host_community_assignment")
)]
pub async fn list_host_community_assignments(
    store: &(dyn HostCommunityAssignmentStore + Send + Sync),
    page: &PageRequest,
    filter: &HostCommunityAssignmentFilter,
) -> Result<Page<HostCommunityAssignment>, AppError> {
    store.list_host_community_assignments(page, filter).await
}

#[tracing::instrument(
    skip(store, audit, events),
    fields(resource_kind = "host_community_assignment")
)]
pub async fn create_host_community_assignment(
    store: &(dyn HostCommunityAssignmentStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateHostCommunityAssignment,
) -> Result<HostCommunityAssignment, AppError> {
    let item = store.create_host_community_assignment(command).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host_community_assignment",
        Some(item.id()),
        item.host_name().as_str(),
        "create",
        json!({"host_name": item.host_name().as_str(), "address": item.address().as_str(), "community_name": item.community_name().as_str(), "policy_name": item.policy_name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(item)
}

#[tracing::instrument(
    level = "debug",
    skip(store),
    fields(resource_kind = "host_community_assignment")
)]
pub async fn get_host_community_assignment(
    store: &(dyn HostCommunityAssignmentStore + Send + Sync),
    mapping_id: Uuid,
) -> Result<HostCommunityAssignment, AppError> {
    store.get_host_community_assignment(mapping_id).await
}

#[tracing::instrument(
    skip(store, audit, events),
    fields(resource_kind = "host_community_assignment")
)]
pub async fn delete_host_community_assignment(
    store: &(dyn HostCommunityAssignmentStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    mapping_id: Uuid,
) -> Result<(), AppError> {
    let old = store.get_host_community_assignment(mapping_id).await?;
    store.delete_host_community_assignment(mapping_id).await?;

    let audit_event = CreateHistoryEvent::new(
        "system",
        "host_community_assignment",
        Some(old.id()),
        old.host_name().as_str(),
        "delete",
        json!({"host_name": old.host_name().as_str(), "address": old.address().as_str(), "community_name": old.community_name().as_str(), "policy_name": old.policy_name().as_str()}),
    );
    super::record_and_emit(audit, events, audit_event).await;

    Ok(())
}
