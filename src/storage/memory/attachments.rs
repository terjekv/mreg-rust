use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashSet;
use uuid::Uuid;

use crate::{
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, HostAttachment,
            UpdateHostAttachment, validate_prefix_reservation_for_attachment,
        },
        community::Community,
        filters::AttachmentCommunityAssignmentFilter,
        pagination::{Page, PageRequest},
        types::{CidrValue, Hostname},
    },
    errors::AppError,
    storage::{AttachmentCommunityAssignmentStore, AttachmentStore},
};

use super::{MemoryState, MemoryStorage, sort_and_paginate};

fn matches_mac_address(
    state: &MemoryState,
    assignment: &AttachmentCommunityAssignment,
    filter: &AttachmentCommunityAssignmentFilter,
) -> bool {
    filter.mac_address.iter().all(|condition| {
        let Some(attachment) = state.host_attachments.get(&assignment.attachment_id()) else {
            return false;
        };
        let Some(mac) = attachment.mac_address() else {
            return false;
        };
        let mac = mac.as_str();
        match condition.op {
            crate::domain::filters::FilterOp::Equals => mac == condition.value,
            crate::domain::filters::FilterOp::IEquals => mac.eq_ignore_ascii_case(&condition.value),
            crate::domain::filters::FilterOp::Contains => mac.contains(&condition.value),
            crate::domain::filters::FilterOp::IContains => mac
                .to_ascii_lowercase()
                .contains(&condition.value.to_ascii_lowercase()),
            crate::domain::filters::FilterOp::StartsWith => mac.starts_with(&condition.value),
            crate::domain::filters::FilterOp::IStartsWith => mac
                .to_ascii_lowercase()
                .starts_with(&condition.value.to_ascii_lowercase()),
            crate::domain::filters::FilterOp::EndsWith => mac.ends_with(&condition.value),
            crate::domain::filters::FilterOp::IEndsWith => mac
                .to_ascii_lowercase()
                .ends_with(&condition.value.to_ascii_lowercase()),
            crate::domain::filters::FilterOp::In => {
                condition.value.split(',').any(|value| value.trim() == mac)
            }
            crate::domain::filters::FilterOp::NotEquals => mac != condition.value,
            crate::domain::filters::FilterOp::NotIEquals => {
                !mac.eq_ignore_ascii_case(&condition.value)
            }
            crate::domain::filters::FilterOp::NotContains => !mac.contains(&condition.value),
            crate::domain::filters::FilterOp::NotIContains => !mac
                .to_ascii_lowercase()
                .contains(&condition.value.to_ascii_lowercase()),
            crate::domain::filters::FilterOp::NotStartsWith => !mac.starts_with(&condition.value),
            crate::domain::filters::FilterOp::NotIStartsWith => !mac
                .to_ascii_lowercase()
                .starts_with(&condition.value.to_ascii_lowercase()),
            crate::domain::filters::FilterOp::NotEndsWith => !mac.ends_with(&condition.value),
            crate::domain::filters::FilterOp::NotIEndsWith => !mac
                .to_ascii_lowercase()
                .ends_with(&condition.value.to_ascii_lowercase()),
            crate::domain::filters::FilterOp::NotIn => {
                !condition.value.split(',').any(|value| value.trim() == mac)
            }
            crate::domain::filters::FilterOp::Gt => mac.as_str() > condition.value.as_str(),
            crate::domain::filters::FilterOp::Gte => mac.as_str() >= condition.value.as_str(),
            crate::domain::filters::FilterOp::Lt => mac.as_str() < condition.value.as_str(),
            crate::domain::filters::FilterOp::Lte => mac.as_str() <= condition.value.as_str(),
            crate::domain::filters::FilterOp::NotGt => mac.as_str() <= condition.value.as_str(),
            crate::domain::filters::FilterOp::NotGte => mac.as_str() < condition.value.as_str(),
            crate::domain::filters::FilterOp::NotLt => mac.as_str() >= condition.value.as_str(),
            crate::domain::filters::FilterOp::NotLte => mac.as_str() > condition.value.as_str(),
            crate::domain::filters::FilterOp::IsNull => false,
            crate::domain::filters::FilterOp::NotIsNull => true,
        }
    })
}

fn community_matches_attachment(community: &Community, attachment: &HostAttachment) -> bool {
    community.network_cidr() == attachment.network_cidr()
}

pub(super) fn create_attachment_in_state(
    state: &mut MemoryState,
    command: CreateHostAttachment,
) -> Result<HostAttachment, AppError> {
    let host = state
        .hosts
        .get(command.host_name().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host '{}' was not found",
                command.host_name().as_str()
            ))
        })?;
    let network = state
        .networks
        .get(&command.network().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "network '{}' was not found",
                command.network().as_str()
            ))
        })?;

    if state.host_attachments.values().any(|attachment| {
        attachment.host_id() == host.id()
            && attachment.network_id() == network.id()
            && attachment.mac_address() == command.mac_address()
    }) {
        return Err(AppError::conflict("host attachment already exists"));
    }

    let now = Utc::now();
    let attachment = HostAttachment::restore(
        Uuid::new_v4(),
        host.id(),
        host.name().clone(),
        network.id(),
        network.cidr().clone(),
        command.mac_address().cloned(),
        command.comment().map(str::to_string),
        now,
        now,
    );
    state
        .host_attachments
        .insert(attachment.id(), attachment.clone());
    Ok(attachment)
}

pub(super) fn find_or_create_attachment_in_state(
    state: &mut MemoryState,
    host_name: &Hostname,
    network: &CidrValue,
    mac_address: Option<crate::domain::types::MacAddressValue>,
) -> Result<HostAttachment, AppError> {
    let host = state
        .hosts
        .get(host_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("host '{}' was not found", host_name.as_str()))
        })?;
    let network_obj = state
        .networks
        .get(&network.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("network '{}' was not found", network.as_str()))
        })?;
    if let Some(existing) = state.host_attachments.values().find(|attachment| {
        attachment.host_id() == host.id()
            && attachment.network_id() == network_obj.id()
            && attachment.mac_address().cloned() == mac_address
    }) {
        return Ok(existing.clone());
    }

    create_attachment_in_state(
        state,
        CreateHostAttachment::new(
            host.name().clone(),
            network_obj.cidr().clone(),
            mac_address,
            None,
        ),
    )
}

pub(super) fn create_attachment_dhcp_identifier_in_state(
    state: &mut MemoryState,
    command: CreateAttachmentDhcpIdentifier,
) -> Result<AttachmentDhcpIdentifier, AppError> {
    if !state
        .host_attachments
        .contains_key(&command.attachment_id())
    {
        return Err(AppError::not_found("host attachment was not found"));
    }
    if state
        .attachment_dhcp_identifiers
        .values()
        .any(|identifier| {
            identifier.attachment_id() == command.attachment_id()
                && identifier.family() == command.family()
                && identifier.kind() == command.kind()
                && identifier.value() == command.value()
        })
    {
        return Err(AppError::conflict(
            "attachment DHCP identifier already exists",
        ));
    }
    let now = Utc::now();
    let identifier = AttachmentDhcpIdentifier::restore(
        Uuid::new_v4(),
        command.attachment_id(),
        command.family(),
        command.kind(),
        command.value(),
        command.priority(),
        now,
        now,
    )?;
    state
        .attachment_dhcp_identifiers
        .insert(identifier.id(), identifier.clone());
    Ok(identifier)
}

pub(super) fn create_attachment_prefix_reservation_in_state(
    state: &mut MemoryState,
    command: CreateAttachmentPrefixReservation,
) -> Result<AttachmentPrefixReservation, AppError> {
    let attachment = state
        .host_attachments
        .get(&command.attachment_id())
        .cloned()
        .ok_or_else(|| AppError::not_found("host attachment was not found"))?;
    validate_prefix_reservation_for_attachment(&attachment, command.prefix())?;
    let now = Utc::now();
    let reservation = AttachmentPrefixReservation::restore(
        Uuid::new_v4(),
        command.attachment_id(),
        command.prefix().clone(),
        now,
        now,
    )?;
    state
        .attachment_prefix_reservations
        .insert(reservation.id(), reservation.clone());
    Ok(reservation)
}

pub(super) fn create_attachment_community_assignment_in_state(
    state: &mut MemoryState,
    command: CreateAttachmentCommunityAssignment,
) -> Result<AttachmentCommunityAssignment, AppError> {
    let attachment = state
        .host_attachments
        .get(&command.attachment_id())
        .cloned()
        .ok_or_else(|| AppError::not_found("host attachment was not found"))?;
    let community = state
        .communities
        .values()
        .find(|community| {
            community.policy_name() == command.policy_name()
                && community.name() == command.community_name()
                && community_matches_attachment(community, &attachment)
        })
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "community '{}/{}' was not found for attachment network",
                command.policy_name().as_str(),
                command.community_name().as_str()
            ))
        })?;
    if state
        .attachment_community_assignments
        .values()
        .any(|assignment| {
            assignment.attachment_id() == attachment.id()
                && assignment.community_id() == community.id()
        })
    {
        return Err(AppError::conflict(
            "attachment community assignment already exists",
        ));
    }
    let now = Utc::now();
    let assignment = AttachmentCommunityAssignment::restore(
        Uuid::new_v4(),
        attachment.id(),
        attachment.host_id(),
        attachment.host_name().clone(),
        attachment.network_id(),
        attachment.network_cidr().clone(),
        community.id(),
        community.name().clone(),
        community.policy_name().clone(),
        now,
        now,
    );
    state
        .attachment_community_assignments
        .insert(assignment.id(), assignment.clone());
    Ok(assignment)
}

#[async_trait]
impl AttachmentStore for MemoryStorage {
    async fn list_attachments(&self, page: &PageRequest) -> Result<Page<HostAttachment>, AppError> {
        let state = self.state.read().await;
        let items: Vec<HostAttachment> = state.host_attachments.values().cloned().collect();
        sort_and_paginate(
            items,
            page,
            &["network", "mac_address"],
            |attachment, field| match field {
                "network" => attachment.network_cidr().as_str(),
                "mac_address" => attachment
                    .mac_address()
                    .map(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
                _ => attachment.host_name().as_str().to_string(),
            },
        )
    }

    async fn list_attachments_for_host(
        &self,
        host: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let state = self.state.read().await;
        Ok(state
            .host_attachments
            .values()
            .filter(|attachment| attachment.host_name() == host)
            .cloned()
            .collect())
    }

    async fn list_attachments_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostAttachment>, AppError> {
        let host_names = hosts
            .iter()
            .map(|host| host.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let state = self.state.read().await;
        Ok(state
            .host_attachments
            .values()
            .filter(|attachment| host_names.contains(attachment.host_name().as_str()))
            .cloned()
            .collect())
    }

    async fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let state = self.state.read().await;
        Ok(state
            .host_attachments
            .values()
            .filter(|attachment| {
                network
                    .as_inner()
                    .contains(attachment.network_cidr().as_inner())
            })
            .cloned()
            .collect())
    }

    async fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        let mut state = self.state.write().await;
        create_attachment_in_state(&mut state, command)
    }

    async fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError> {
        let state = self.state.read().await;
        state
            .host_attachments
            .get(&attachment_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("host attachment was not found"))
    }

    async fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        let mut state = self.state.write().await;
        let existing = state
            .host_attachments
            .get(&attachment_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("host attachment was not found"))?;
        let now = Utc::now();
        let updated = HostAttachment::restore(
            existing.id(),
            existing.host_id(),
            existing.host_name().clone(),
            existing.network_id(),
            existing.network_cidr().clone(),
            command.mac_address.resolve(existing.mac_address().cloned()),
            command
                .comment
                .resolve(existing.comment().map(str::to_string)),
            existing.created_at(),
            now,
        );
        state.host_attachments.insert(updated.id(), updated.clone());
        Ok(updated)
    }

    async fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        if state
            .ip_addresses
            .values()
            .any(|assignment| assignment.attachment_id() == attachment_id)
        {
            return Err(AppError::conflict(
                "host attachment still owns IP address reservations",
            ));
        }
        state
            .host_attachments
            .remove(&attachment_id)
            .ok_or_else(|| AppError::not_found("host attachment was not found"))?;
        state
            .attachment_dhcp_identifiers
            .retain(|_, identifier| identifier.attachment_id() != attachment_id);
        state
            .attachment_prefix_reservations
            .retain(|_, reservation| reservation.attachment_id() != attachment_id);
        state
            .attachment_community_assignments
            .retain(|_, assignment| assignment.attachment_id() != attachment_id);
        Ok(())
    }

    async fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        let state = self.state.read().await;
        Ok(state
            .attachment_dhcp_identifiers
            .values()
            .filter(|identifier| identifier.attachment_id() == attachment_id)
            .cloned()
            .collect())
    }

    async fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        let state = self.state.read().await;
        let attachment_ids = attachment_ids.iter().copied().collect::<HashSet<_>>();
        Ok(state
            .attachment_dhcp_identifiers
            .values()
            .filter(|identifier| attachment_ids.contains(&identifier.attachment_id()))
            .cloned()
            .collect())
    }

    async fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        let mut state = self.state.write().await;
        create_attachment_dhcp_identifier_in_state(&mut state, command)
    }

    async fn delete_attachment_dhcp_identifier(&self, identifier_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .attachment_dhcp_identifiers
            .remove(&identifier_id)
            .ok_or_else(|| AppError::not_found("attachment DHCP identifier was not found"))?;
        Ok(())
    }

    async fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        let state = self.state.read().await;
        Ok(state
            .attachment_prefix_reservations
            .values()
            .filter(|reservation| reservation.attachment_id() == attachment_id)
            .cloned()
            .collect())
    }

    async fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        let state = self.state.read().await;
        let attachment_ids = attachment_ids.iter().copied().collect::<HashSet<_>>();
        Ok(state
            .attachment_prefix_reservations
            .values()
            .filter(|reservation| attachment_ids.contains(&reservation.attachment_id()))
            .cloned()
            .collect())
    }

    async fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        let mut state = self.state.write().await;
        create_attachment_prefix_reservation_in_state(&mut state, command)
    }

    async fn delete_attachment_prefix_reservation(
        &self,
        reservation_id: Uuid,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .attachment_prefix_reservations
            .remove(&reservation_id)
            .ok_or_else(|| AppError::not_found("attachment prefix reservation was not found"))?;
        Ok(())
    }
}

#[async_trait]
impl AttachmentCommunityAssignmentStore for MemoryStorage {
    async fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        let state = self.state.read().await;
        let items: Vec<AttachmentCommunityAssignment> = state
            .attachment_community_assignments
            .values()
            .filter(|assignment| {
                filter.matches(assignment) && matches_mac_address(&state, assignment, filter)
            })
            .cloned()
            .collect();
        sort_and_paginate(
            items,
            page,
            &["network", "policy_name", "community_name"],
            |assignment, field| match field {
                "network" => assignment.network_cidr().as_str(),
                "policy_name" => assignment.policy_name().as_str().to_string(),
                "community_name" => assignment.community_name().as_str().to_string(),
                _ => assignment.host_name().as_str().to_string(),
            },
        )
    }

    async fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        let state = self.state.read().await;
        let attachment_ids = attachment_ids.iter().copied().collect::<HashSet<_>>();
        Ok(state
            .attachment_community_assignments
            .values()
            .filter(|assignment| attachment_ids.contains(&assignment.attachment_id()))
            .cloned()
            .collect())
    }

    async fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        let mut state = self.state.write().await;
        create_attachment_community_assignment_in_state(&mut state, command)
    }

    async fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        let state = self.state.read().await;
        state
            .attachment_community_assignments
            .get(&assignment_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("attachment community assignment was not found"))
    }

    async fn delete_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .attachment_community_assignments
            .remove(&assignment_id)
            .ok_or_else(|| AppError::not_found("attachment community assignment was not found"))?;
        Ok(())
    }
}
