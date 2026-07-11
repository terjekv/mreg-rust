use uuid::Uuid;

use crate::{
    domain::{
        attachment::{
            AttachmentDhcpIdentifier, AttachmentPrefixReservation, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, HostAttachment,
            UpdateHostAttachment,
        },
        pagination::{Page, PageRequest},
        types::{CidrValue, Hostname},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::AttachmentStore`].
pub trait TxAttachmentStore {
    fn list_attachments(&self, page: &PageRequest) -> Result<Page<HostAttachment>, AppError>;
    fn list_attachments_for_host(
        &self,
        host: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError>;
    fn list_attachments_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostAttachment>, AppError>;
    fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError>;
    fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError>;
    fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError>;
    fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError>;
    fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError>;

    fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError>;
    fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError>;
    fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError>;
    fn delete_attachment_dhcp_identifier(&self, identifier_id: Uuid) -> Result<(), AppError>;

    fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError>;
    fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError>;
    fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError>;
    fn delete_attachment_prefix_reservation(&self, reservation_id: Uuid)
    -> Result<(), AppError>;
}
