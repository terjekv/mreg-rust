use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, HostCommunityAssignmentStore},
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
    skip(storage, events),
    fields(resource_kind = "host_community_assignment")
)]
pub async fn create_host_community_assignment(
    storage: &DynStorage,
    command: CreateHostCommunityAssignment,
    events: &EventSinkClient,
) -> Result<HostCommunityAssignment, AppError> {
    let (item, history) = storage
        .transaction(move |tx| {
            if env_flag("MREG_REQUIRE_MAC_FOR_BINDING_IP_TO_COMMUNITY") {
                let address = tx
                    .hosts()
                    .list_ip_addresses_for_host(command.host_name(), &PageRequest::all())?
                    .items
                    .into_iter()
                    .find(|assignment| assignment.address() == command.address())
                    .ok_or_else(|| AppError::not_found("IP address was not found for host"))?;
                if address.mac_address().is_none() {
                    return Err(AppError::not_acceptable(
                        "The IP must have a MAC address to bind it to a community.",
                    ));
                }
            }
            let existing = tx
                .host_community_assignments()
                .list_host_community_assignments(
                    &PageRequest::all(),
                    &HostCommunityAssignmentFilter::default(),
                )?
                .items
                .into_iter()
                .filter(|assignment| {
                    assignment.host_name() == command.host_name()
                        && assignment.address() == command.address()
                })
                .map(|assignment| assignment.id())
                .collect::<Vec<_>>();
            for assignment_id in existing {
                tx.host_community_assignments()
                    .delete_host_community_assignment(assignment_id)?;
            }
            let item = tx
                .host_community_assignments()
                .create_host_community_assignment(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "host_community_assignment",
                Some(item.id()),
                item.host_name().as_str(),
                actions::CREATE,
                json!({"host_name": item.host_name().as_str(), "address": item.address().as_str(), "community_name": item.community_name().as_str(), "policy_name": item.policy_name().as_str()}),
            ))?;
            Ok((item, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(item)
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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
    skip(storage, events),
    fields(resource_kind = "host_community_assignment")
)]
pub async fn delete_host_community_assignment(
    storage: &DynStorage,
    mapping_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx
                .host_community_assignments()
                .get_host_community_assignment(mapping_id)?;
            tx.host_community_assignments()
                .delete_host_community_assignment(mapping_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::current(),
                "host_community_assignment",
                Some(old.id()),
                old.host_name().as_str(),
                actions::DELETE,
                json!({"host_name": old.host_name().as_str(), "address": old.address().as_str(), "community_name": old.community_name().as_str(), "policy_name": old.policy_name().as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
