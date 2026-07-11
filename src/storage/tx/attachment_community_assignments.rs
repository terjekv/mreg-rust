use uuid::Uuid;

use crate::{
    domain::{
        attachment::{AttachmentCommunityAssignment, CreateAttachmentCommunityAssignment},
        filters::AttachmentCommunityAssignmentFilter,
        pagination::{Page, PageRequest},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of
/// [`crate::storage::AttachmentCommunityAssignmentStore`].
pub trait TxAttachmentCommunityAssignmentStore {
    fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError>;
    fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError>;
    fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError>;
    fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError>;
    fn delete_attachment_community_assignment(&self, assignment_id: Uuid)
    -> Result<(), AppError>;
}
