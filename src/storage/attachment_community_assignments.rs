use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    domain::{
        attachment::{AttachmentCommunityAssignment, CreateAttachmentCommunityAssignment},
        filters::AttachmentCommunityAssignmentFilter,
        pagination::{Page, PageRequest},
    },
    errors::AppError,
};

/// CRUD operations for attachment-scoped community assignments.
#[async_trait]
pub trait AttachmentCommunityAssignmentStore: Send + Sync {
    async fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError>;
    async fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError>;
    async fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError>;
    async fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError>;
    async fn delete_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<(), AppError>;
}
