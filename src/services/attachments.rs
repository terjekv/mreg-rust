use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::CreateHistoryEvent,
    domain::attachment::{
        AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
        CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
        CreateAttachmentPrefixReservation, CreateHostAttachment, HostAttachment,
        UpdateHostAttachment,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AttachmentCommunityAssignmentStore, AttachmentStore, AuditStore},
};

#[tracing::instrument(
    level = "debug",
    skip(store, audit, events),
    fields(resource_kind = "host_attachment")
)]
pub async fn create_attachment(
    store: &(dyn AttachmentStore + Send + Sync),
    command: CreateHostAttachment,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<HostAttachment, AppError> {
    let attachment = store.create_attachment(command).await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "host_attachment",
            Some(attachment.id()),
            attachment.host_name().as_str(),
            "create",
            json!({
                "host_name": attachment.host_name().as_str(),
                "network": attachment.network_cidr().as_str(),
                "mac_address": attachment.mac_address().map(|value| value.as_str()),
            }),
        ),
    )
    .await;
    Ok(attachment)
}

pub async fn update_attachment(
    store: &(dyn AttachmentStore + Send + Sync),
    attachment_id: Uuid,
    command: UpdateHostAttachment,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<HostAttachment, AppError> {
    let attachment = store.update_attachment(attachment_id, command).await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "host_attachment",
            Some(attachment.id()),
            attachment.host_name().as_str(),
            "update",
            json!({
                "host_name": attachment.host_name().as_str(),
                "network": attachment.network_cidr().as_str(),
                "mac_address": attachment.mac_address().map(|value| value.as_str()),
            }),
        ),
    )
    .await;
    Ok(attachment)
}

pub async fn delete_attachment(
    store: &(dyn AttachmentStore + Send + Sync),
    attachment_id: Uuid,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let old = store.get_attachment(attachment_id).await?;
    store.delete_attachment(attachment_id).await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "host_attachment",
            Some(old.id()),
            old.host_name().as_str(),
            "delete",
            json!({
                "host_name": old.host_name().as_str(),
                "network": old.network_cidr().as_str(),
            }),
        ),
    )
    .await;
    Ok(())
}

pub async fn create_attachment_dhcp_identifier(
    store: &(dyn AttachmentStore + Send + Sync),
    command: CreateAttachmentDhcpIdentifier,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<AttachmentDhcpIdentifier, AppError> {
    let identifier = store.create_attachment_dhcp_identifier(command).await?;
    let kind = match identifier.kind() {
        crate::domain::attachment::DhcpIdentifierKind::ClientId => "client_id",
        crate::domain::attachment::DhcpIdentifierKind::DuidLlt => "duid_llt",
        crate::domain::attachment::DhcpIdentifierKind::DuidEn => "duid_en",
        crate::domain::attachment::DhcpIdentifierKind::DuidLl => "duid_ll",
        crate::domain::attachment::DhcpIdentifierKind::DuidUuid => "duid_uuid",
        crate::domain::attachment::DhcpIdentifierKind::DuidRaw => "duid_raw",
    };
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "attachment_dhcp_identifier",
            Some(identifier.id()),
            identifier.value(),
            "create",
            json!({
                "attachment_id": identifier.attachment_id(),
                "family": identifier.family().as_u8(),
                "kind": kind,
                "value": identifier.value(),
            }),
        ),
    )
    .await;
    Ok(identifier)
}

pub async fn delete_attachment_dhcp_identifier(
    store: &(dyn AttachmentStore + Send + Sync),
    attachment_id: Uuid,
    identifier_id: Uuid,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let identifier = store
        .list_attachment_dhcp_identifiers(attachment_id)
        .await?
        .into_iter()
        .find(|item| item.id() == identifier_id)
        .ok_or_else(|| AppError::not_found("attachment DHCP identifier was not found"))?;
    store
        .delete_attachment_dhcp_identifier(identifier_id)
        .await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "attachment_dhcp_identifier",
            Some(identifier.id()),
            identifier.value(),
            "delete",
            json!({"attachment_id": identifier.attachment_id(), "value": identifier.value()}),
        ),
    )
    .await;
    Ok(())
}

pub async fn create_attachment_prefix_reservation(
    store: &(dyn AttachmentStore + Send + Sync),
    command: CreateAttachmentPrefixReservation,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<AttachmentPrefixReservation, AppError> {
    let reservation = store.create_attachment_prefix_reservation(command).await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "attachment_prefix_reservation",
            Some(reservation.id()),
            reservation.prefix().as_str(),
            "create",
            json!({"attachment_id": reservation.attachment_id(), "prefix": reservation.prefix().as_str()}),
        ),
    )
    .await;
    Ok(reservation)
}

pub async fn delete_attachment_prefix_reservation(
    store: &(dyn AttachmentStore + Send + Sync),
    attachment_id: Uuid,
    reservation_id: Uuid,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let reservation = store
        .list_attachment_prefix_reservations(attachment_id)
        .await?
        .into_iter()
        .find(|item| item.id() == reservation_id)
        .ok_or_else(|| AppError::not_found("attachment prefix reservation was not found"))?;
    store
        .delete_attachment_prefix_reservation(reservation_id)
        .await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "attachment_prefix_reservation",
            Some(reservation.id()),
            reservation.prefix().as_str(),
            "delete",
            json!({"attachment_id": reservation.attachment_id(), "prefix": reservation.prefix().as_str()}),
        ),
    )
    .await;
    Ok(())
}

pub async fn create_attachment_community_assignment(
    store: &(dyn AttachmentCommunityAssignmentStore + Send + Sync),
    command: CreateAttachmentCommunityAssignment,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<AttachmentCommunityAssignment, AppError> {
    let assignment = store
        .create_attachment_community_assignment(command)
        .await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "attachment_community_assignment",
            Some(assignment.id()),
            assignment.host_name().as_str(),
            "create",
            json!({
                "attachment_id": assignment.attachment_id(),
                "host_name": assignment.host_name().as_str(),
                "network": assignment.network_cidr().as_str(),
                "policy_name": assignment.policy_name().as_str(),
                "community_name": assignment.community_name().as_str(),
            }),
        ),
    )
    .await;
    Ok(assignment)
}

pub async fn delete_attachment_community_assignment(
    store: &(dyn AttachmentCommunityAssignmentStore + Send + Sync),
    assignment_id: Uuid,
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let old = store
        .get_attachment_community_assignment(assignment_id)
        .await?;
    store
        .delete_attachment_community_assignment(assignment_id)
        .await?;
    super::record_and_emit(
        audit,
        events,
        CreateHistoryEvent::new(
            "system",
            "attachment_community_assignment",
            Some(old.id()),
            old.host_name().as_str(),
            "delete",
            json!({
                "attachment_id": old.attachment_id(),
                "host_name": old.host_name().as_str(),
                "network": old.network_cidr().as_str(),
                "policy_name": old.policy_name().as_str(),
                "community_name": old.community_name().as_str(),
            }),
        ),
    )
    .await;
    Ok(())
}
