use crate::{
    domain::{
        community::{Community, CreateCommunity},
        filters::CommunityFilter,
        pagination::{Page, PageRequest},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::CommunityStore`].
pub trait TxCommunityStore {
    fn list_communities(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError>;
    fn create_community(&self, command: CreateCommunity) -> Result<Community, AppError>;
    fn get_community(&self, community_id: uuid::Uuid) -> Result<Community, AppError>;
    fn delete_community(&self, community_id: uuid::Uuid) -> Result<(), AppError>;
    fn find_community_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError>;
}
