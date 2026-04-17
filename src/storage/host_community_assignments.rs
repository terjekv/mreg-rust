use async_trait::async_trait;

use crate::{
    domain::{
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        pagination::{Page, PageRequest},
    },
    errors::AppError,
};

/// CRUD operations for host-community assignments.
#[async_trait]
pub trait HostCommunityAssignmentStore: Send + Sync {
    async fn list_host_community_assignments(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError>;
    async fn create_host_community_assignment(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError>;
    async fn get_host_community_assignment(
        &self,
        mapping_id: uuid::Uuid,
    ) -> Result<HostCommunityAssignment, AppError>;
    async fn delete_host_community_assignment(
        &self,
        mapping_id: uuid::Uuid,
    ) -> Result<(), AppError>;
}
