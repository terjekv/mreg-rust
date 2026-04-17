use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        community::{Community, CreateCommunity},
        filters::CommunityFilter,
        pagination::{Page, PageRequest},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
    storage::CommunityStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

pub(super) fn create_community_in_state(
    state: &mut MemoryState,
    command: CreateCommunity,
) -> Result<Community, AppError> {
    let policy = state
        .network_policies
        .get(command.policy_name().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "network policy '{}' was not found",
                command.policy_name().as_str()
            ))
        })?;
    if !state
        .networks
        .contains_key(&command.network_cidr().as_str())
    {
        return Err(AppError::not_found(format!(
            "network '{}' was not found",
            command.network_cidr().as_str()
        )));
    }
    if state.communities.values().any(|community| {
        community.policy_name() == command.policy_name() && community.name() == command.name()
    }) {
        return Err(AppError::conflict(format!(
            "community '{}:{}' already exists",
            command.policy_name().as_str(),
            command.name().as_str()
        )));
    }
    let now = Utc::now();
    let community = Community::restore(
        Uuid::new_v4(),
        policy.id(),
        command.policy_name().clone(),
        command.network_cidr().clone(),
        command.name().clone(),
        command.description().to_string(),
        now,
        now,
    )?;
    state.communities.insert(community.id(), community.clone());
    Ok(community)
}

#[async_trait]
impl CommunityStore for MemoryStorage {
    async fn list_communities(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<Community> = state
            .communities
            .values()
            .filter(|community| filter.matches(community))
            .cloned()
            .collect();
        sort_items(&mut items, page, |community, field| match field {
            "policy_name" => community.policy_name().as_str().to_string(),
            "created_at" => community.created_at().to_rfc3339(),
            _ => community.name().as_str().to_string(),
        });
        paginate_by_cursor(items, page)
    }

    async fn create_community(&self, command: CreateCommunity) -> Result<Community, AppError> {
        let mut state = self.state.write().await;
        create_community_in_state(&mut state, command)
    }

    async fn get_community(&self, community_id: Uuid) -> Result<Community, AppError> {
        let state = self.state.read().await;
        state
            .communities
            .get(&community_id)
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("community '{}' was not found", community_id))
            })
    }

    async fn delete_community(&self, community_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .communities
            .remove(&community_id)
            .map(|_| ())
            .ok_or_else(|| {
                AppError::not_found(format!("community '{}' was not found", community_id))
            })
    }

    async fn find_community_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError> {
        let state = self.state.read().await;
        state
            .communities
            .values()
            .find(|community| {
                community.policy_name() == policy_name && community.name() == community_name
            })
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "community '{}:{}' was not found",
                    policy_name.as_str(),
                    community_name.as_str()
                ))
            })
    }
}
