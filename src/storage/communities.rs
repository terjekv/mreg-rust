use async_trait::async_trait;

use crate::{
    domain::{
        community::{Community, CreateCommunity},
        filters::CommunityFilter,
        pagination::{Page, PageRequest},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
};

/// CRUD operations for communities.
#[async_trait]
pub trait CommunityStore: Send + Sync {
    async fn list_communities(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError>;
    async fn create_community(&self, command: CreateCommunity) -> Result<Community, AppError>;
    async fn get_community(&self, community_id: uuid::Uuid) -> Result<Community, AppError>;
    async fn delete_community(&self, community_id: uuid::Uuid) -> Result<(), AppError>;
    async fn find_community_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError>;
}
