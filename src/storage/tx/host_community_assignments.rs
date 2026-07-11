use crate::{
    domain::{
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of
/// [`crate::storage::HostCommunityAssignmentStore`].
pub trait TxHostCommunityAssignmentStore {
    fn list_host_community_assignments(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError>;
    fn create_host_community_assignment(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError>;
    fn get_host_community_assignment(
        &self,
        mapping_id: uuid::Uuid,
    ) -> Result<HostCommunityAssignment, AppError>;
    fn delete_host_community_assignment(&self, mapping_id: uuid::Uuid) -> Result<(), AppError>;
}
