use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
    storage::HostCommunityAssignmentStore,
};

use super::{MemoryState, MemoryStorage, sort_and_paginate};

pub(super) fn create_host_community_assignment_in_state(
    state: &mut MemoryState,
    command: CreateHostCommunityAssignment,
) -> Result<HostCommunityAssignment, AppError> {
    let host = state
        .hosts
        .get(command.host_name().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host '{}' was not found",
                command.host_name().as_str()
            ))
        })?;
    let assignment = state
        .ip_addresses
        .get(&command.address().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "ip address '{}' was not found",
                command.address().as_str()
            ))
        })?;
    if assignment.host_id() != host.id() {
        return Err(AppError::validation(
            "host community assignment address must belong to the supplied host",
        ));
    }
    let community = state
        .communities
        .values()
        .find(|community| {
            community.policy_name() == command.policy_name()
                && community.name() == command.community_name()
        })
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "community '{}:{}' was not found",
                command.policy_name().as_str(),
                command.community_name().as_str()
            ))
        })?;
    if state.host_community_assignments.values().any(|mapping| {
        mapping.host_id() == host.id()
            && mapping.ip_address_id() == assignment.id()
            && mapping.community_id() == community.id()
    }) {
        return Err(AppError::conflict(
            "host community assignment already exists",
        ));
    }
    let now = Utc::now();
    let mapping = HostCommunityAssignment::restore(
        Uuid::new_v4(),
        host.id(),
        host.name().clone(),
        assignment.id(),
        *assignment.address(),
        community.id(),
        community.name().clone(),
        community.policy_name().clone(),
        now,
        now,
    );
    state
        .host_community_assignments
        .insert(mapping.id(), mapping.clone());
    Ok(mapping)
}

#[async_trait]
impl HostCommunityAssignmentStore for MemoryStorage {
    async fn list_host_community_assignments(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError> {
        let state = self.state.read().await;
        let items: Vec<HostCommunityAssignment> = state
            .host_community_assignments
            .values()
            .filter(|mapping| filter.matches(mapping))
            .cloned()
            .collect();
        sort_and_paginate(
            items,
            page,
            &["community_name", "created_at"],
            |mapping, field| match field {
                "community_name" => mapping.community_name().as_str().to_string(),
                "created_at" => mapping.created_at().to_rfc3339(),
                _ => mapping.host_name().as_str().to_string(),
            },
        )
    }

    async fn create_host_community_assignment(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError> {
        let mut state = self.state.write().await;
        create_host_community_assignment_in_state(&mut state, command)
    }

    async fn get_host_community_assignment(
        &self,
        mapping_id: Uuid,
    ) -> Result<HostCommunityAssignment, AppError> {
        let state = self.state.read().await;
        state
            .host_community_assignments
            .get(&mapping_id)
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "host community assignment '{}' was not found",
                    mapping_id
                ))
            })
    }

    async fn delete_host_community_assignment(&self, mapping_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .host_community_assignments
            .remove(&mapping_id)
            .map(|_| ())
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "host community assignment '{}' was not found",
                    mapping_id
                ))
            })
    }
}
