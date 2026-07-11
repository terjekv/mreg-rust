use serde_json::json;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::attachment::{
        AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
        CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
        CreateAttachmentPrefixReservation, CreateHostAttachment, HostAttachment,
        UpdateHostAttachment,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::DynStorage,
};

#[tracing::instrument(
    level = "debug",
    skip(storage, events),
    fields(resource_kind = "host_attachment")
)]
pub async fn create_attachment(
    storage: &DynStorage,
    command: CreateHostAttachment,
    events: &EventSinkClient,
) -> Result<HostAttachment, AppError> {
    let (attachment, history) = storage
        .transaction(move |tx| {
            let attachment = tx.attachments().create_attachment(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_attachment",
                Some(attachment.id()),
                attachment.host_name().as_str(),
                actions::CREATE,
                json!({
                    "host_name": attachment.host_name().as_str(),
                    "network": attachment.network_cidr().as_str(),
                    "mac_address": attachment.mac_address().map(|value| value.as_str()),
                }),
            ))?;
            Ok((attachment, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(attachment)
}

pub async fn update_attachment(
    storage: &DynStorage,
    attachment_id: Uuid,
    command: UpdateHostAttachment,
    events: &EventSinkClient,
) -> Result<HostAttachment, AppError> {
    let (attachment, history) = storage
        .transaction(move |tx| {
            let attachment = tx.attachments().update_attachment(attachment_id, command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_attachment",
                Some(attachment.id()),
                attachment.host_name().as_str(),
                actions::UPDATE,
                json!({
                    "host_name": attachment.host_name().as_str(),
                    "network": attachment.network_cidr().as_str(),
                    "mac_address": attachment.mac_address().map(|value| value.as_str()),
                }),
            ))?;
            Ok((attachment, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(attachment)
}

pub async fn delete_attachment(
    storage: &DynStorage,
    attachment_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx.attachments().get_attachment(attachment_id)?;
            tx.attachments().delete_attachment(attachment_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_attachment",
                Some(old.id()),
                old.host_name().as_str(),
                actions::DELETE,
                json!({
                    "host_name": old.host_name().as_str(),
                    "network": old.network_cidr().as_str(),
                }),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

pub async fn create_attachment_dhcp_identifier(
    storage: &DynStorage,
    command: CreateAttachmentDhcpIdentifier,
    events: &EventSinkClient,
) -> Result<AttachmentDhcpIdentifier, AppError> {
    let (identifier, history) = storage
        .transaction(move |tx| {
            let identifier = tx.attachments().create_attachment_dhcp_identifier(command)?;
            let kind = match identifier.kind() {
                crate::domain::attachment::DhcpIdentifierKind::ClientId => "client_id",
                crate::domain::attachment::DhcpIdentifierKind::DuidLlt => "duid_llt",
                crate::domain::attachment::DhcpIdentifierKind::DuidEn => "duid_en",
                crate::domain::attachment::DhcpIdentifierKind::DuidLl => "duid_ll",
                crate::domain::attachment::DhcpIdentifierKind::DuidUuid => "duid_uuid",
                crate::domain::attachment::DhcpIdentifierKind::DuidRaw => "duid_raw",
            };
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "attachment_dhcp_identifier",
                Some(identifier.id()),
                identifier.value(),
                actions::CREATE,
                json!({
                    "attachment_id": identifier.attachment_id(),
                    "family": identifier.family().as_u8(),
                    "kind": kind,
                    "value": identifier.value(),
                }),
            ))?;
            Ok((identifier, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(identifier)
}

pub async fn delete_attachment_dhcp_identifier(
    storage: &DynStorage,
    attachment_id: Uuid,
    identifier_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let identifier = tx
                .attachments()
                .list_attachment_dhcp_identifiers(attachment_id)?
                .into_iter()
                .find(|item| item.id() == identifier_id)
                .ok_or_else(|| {
                    AppError::not_found("attachment DHCP identifier was not found")
                })?;
            tx.attachments()
                .delete_attachment_dhcp_identifier(identifier_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "attachment_dhcp_identifier",
                Some(identifier.id()),
                identifier.value(),
                actions::DELETE,
                json!({"attachment_id": identifier.attachment_id(), "value": identifier.value()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

pub async fn create_attachment_prefix_reservation(
    storage: &DynStorage,
    command: CreateAttachmentPrefixReservation,
    events: &EventSinkClient,
) -> Result<AttachmentPrefixReservation, AppError> {
    let (reservation, history) = storage
        .transaction(move |tx| {
            let reservation = tx
                .attachments()
                .create_attachment_prefix_reservation(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "attachment_prefix_reservation",
                Some(reservation.id()),
                reservation.prefix().as_str(),
                actions::CREATE,
                json!({"attachment_id": reservation.attachment_id(), "prefix": reservation.prefix().as_str()}),
            ))?;
            Ok((reservation, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(reservation)
}

pub async fn delete_attachment_prefix_reservation(
    storage: &DynStorage,
    attachment_id: Uuid,
    reservation_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let reservation = tx
                .attachments()
                .list_attachment_prefix_reservations(attachment_id)?
                .into_iter()
                .find(|item| item.id() == reservation_id)
                .ok_or_else(|| {
                    AppError::not_found("attachment prefix reservation was not found")
                })?;
            tx.attachments()
                .delete_attachment_prefix_reservation(reservation_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "attachment_prefix_reservation",
                Some(reservation.id()),
                reservation.prefix().as_str(),
                actions::DELETE,
                json!({"attachment_id": reservation.attachment_id(), "prefix": reservation.prefix().as_str()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}

pub async fn create_attachment_community_assignment(
    storage: &DynStorage,
    command: CreateAttachmentCommunityAssignment,
    events: &EventSinkClient,
) -> Result<AttachmentCommunityAssignment, AppError> {
    let (assignment, history) = storage
        .transaction(move |tx| {
            let assignment = tx
                .attachment_community_assignments()
                .create_attachment_community_assignment(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "attachment_community_assignment",
                Some(assignment.id()),
                assignment.host_name().as_str(),
                actions::CREATE,
                json!({
                    "attachment_id": assignment.attachment_id(),
                    "host_name": assignment.host_name().as_str(),
                    "network": assignment.network_cidr().as_str(),
                    "policy_name": assignment.policy_name().as_str(),
                    "community_name": assignment.community_name().as_str(),
                }),
            ))?;
            Ok((assignment, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(assignment)
}

pub async fn delete_attachment_community_assignment(
    storage: &DynStorage,
    assignment_id: Uuid,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let history = storage
        .transaction(move |tx| {
            let old = tx
                .attachment_community_assignments()
                .get_attachment_community_assignment(assignment_id)?;
            tx.attachment_community_assignments()
                .delete_attachment_community_assignment(assignment_id)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "attachment_community_assignment",
                Some(old.id()),
                old.host_name().as_str(),
                actions::DELETE,
                json!({
                    "attachment_id": old.attachment_id(),
                    "host_name": old.host_name().as_str(),
                    "network": old.network_cidr().as_str(),
                    "policy_name": old.policy_name().as_str(),
                    "community_name": old.community_name().as_str(),
                }),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
