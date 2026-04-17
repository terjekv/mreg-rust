use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    domain::types::{CidrValue, CommunityName, Hostname, MacAddressValue, NetworkPolicyName},
    errors::AppError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DhcpIdentifierFamily {
    V4,
    V6,
}

impl DhcpIdentifierFamily {
    pub fn as_u8(self) -> u8 {
        match self {
            Self::V4 => 4,
            Self::V6 => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DhcpIdentifierKind {
    ClientId,
    DuidLlt,
    DuidEn,
    DuidLl,
    DuidUuid,
    DuidRaw,
}

impl DhcpIdentifierKind {
    fn validate_for_family(self, family: DhcpIdentifierFamily) -> Result<(), AppError> {
        match (family, self) {
            (DhcpIdentifierFamily::V4, DhcpIdentifierKind::ClientId) => Ok(()),
            (DhcpIdentifierFamily::V6, DhcpIdentifierKind::DuidLlt)
            | (DhcpIdentifierFamily::V6, DhcpIdentifierKind::DuidEn)
            | (DhcpIdentifierFamily::V6, DhcpIdentifierKind::DuidLl)
            | (DhcpIdentifierFamily::V6, DhcpIdentifierKind::DuidUuid)
            | (DhcpIdentifierFamily::V6, DhcpIdentifierKind::DuidRaw) => Ok(()),
            (DhcpIdentifierFamily::V4, _) => Err(AppError::validation(
                "dhcpv4 identifiers only support client_id",
            )),
            (DhcpIdentifierFamily::V6, _) => Err(AppError::validation(
                "dhcpv6 identifiers only support DUID kinds",
            )),
        }
    }
}

/// Attachment of a host to a network, optionally keyed by a MAC address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostAttachment {
    id: Uuid,
    host_id: Uuid,
    host_name: Hostname,
    network_id: Uuid,
    network_cidr: CidrValue,
    mac_address: Option<MacAddressValue>,
    comment: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostAttachment {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        host_id: Uuid,
        host_name: Hostname,
        network_id: Uuid,
        network_cidr: CidrValue,
        mac_address: Option<MacAddressValue>,
        comment: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            host_id,
            host_name,
            network_id,
            network_cidr,
            mac_address,
            comment: normalize_optional_text(comment),
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }

    pub fn network_id(&self) -> Uuid {
        self.network_id
    }

    pub fn network_cidr(&self) -> &CidrValue {
        &self.network_cidr
    }

    pub fn mac_address(&self) -> Option<&MacAddressValue> {
        self.mac_address.as_ref()
    }

    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateHostAttachment {
    host_name: Hostname,
    network: CidrValue,
    mac_address: Option<MacAddressValue>,
    comment: Option<String>,
}

impl CreateHostAttachment {
    pub fn new(
        host_name: Hostname,
        network: CidrValue,
        mac_address: Option<MacAddressValue>,
        comment: Option<String>,
    ) -> Self {
        Self {
            host_name,
            network,
            mac_address,
            comment: normalize_optional_text(comment),
        }
    }

    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }

    pub fn network(&self) -> &CidrValue {
        &self.network
    }

    pub fn mac_address(&self) -> Option<&MacAddressValue> {
        self.mac_address.as_ref()
    }

    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }
}

#[derive(Clone, Debug)]
pub struct UpdateHostAttachment {
    pub mac_address: Option<Option<MacAddressValue>>,
    pub comment: Option<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentDhcpIdentifier {
    id: Uuid,
    attachment_id: Uuid,
    family: DhcpIdentifierFamily,
    kind: DhcpIdentifierKind,
    value: String,
    priority: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttachmentDhcpIdentifier {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        attachment_id: Uuid,
        family: DhcpIdentifierFamily,
        kind: DhcpIdentifierKind,
        value: impl Into<String>,
        priority: i32,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        kind.validate_for_family(family)?;
        let value = normalize_required_text(value.into(), "dhcp identifier value")?;
        Ok(Self {
            id,
            attachment_id,
            family,
            kind,
            value: normalize_identifier_value(&value),
            priority,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn family(&self) -> DhcpIdentifierFamily {
        self.family
    }

    pub fn kind(&self) -> DhcpIdentifierKind {
        self.kind
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn priority(&self) -> i32 {
        self.priority
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateAttachmentDhcpIdentifier {
    attachment_id: Uuid,
    family: DhcpIdentifierFamily,
    kind: DhcpIdentifierKind,
    value: String,
    priority: i32,
}

impl CreateAttachmentDhcpIdentifier {
    pub fn new(
        attachment_id: Uuid,
        family: DhcpIdentifierFamily,
        kind: DhcpIdentifierKind,
        value: impl Into<String>,
        priority: i32,
    ) -> Result<Self, AppError> {
        kind.validate_for_family(family)?;
        let value = normalize_required_text(value.into(), "dhcp identifier value")?;
        Ok(Self {
            attachment_id,
            family,
            kind,
            value: normalize_identifier_value(&value),
            priority,
        })
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn family(&self) -> DhcpIdentifierFamily {
        self.family
    }

    pub fn kind(&self) -> DhcpIdentifierKind {
        self.kind
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn priority(&self) -> i32 {
        self.priority
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentPrefixReservation {
    id: Uuid,
    attachment_id: Uuid,
    prefix: CidrValue,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttachmentPrefixReservation {
    pub fn restore(
        id: Uuid,
        attachment_id: Uuid,
        prefix: CidrValue,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        if !prefix.is_v6() {
            return Err(AppError::validation(
                "attachment prefix reservations must be IPv6 prefixes",
            ));
        }
        Ok(Self {
            id,
            attachment_id,
            prefix,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn prefix(&self) -> &CidrValue {
        &self.prefix
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateAttachmentPrefixReservation {
    attachment_id: Uuid,
    prefix: CidrValue,
}

impl CreateAttachmentPrefixReservation {
    pub fn new(attachment_id: Uuid, prefix: CidrValue) -> Result<Self, AppError> {
        if !prefix.is_v6() {
            return Err(AppError::validation(
                "attachment prefix reservations must be IPv6 prefixes",
            ));
        }
        Ok(Self {
            attachment_id,
            prefix,
        })
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn prefix(&self) -> &CidrValue {
        &self.prefix
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentCommunityAssignment {
    id: Uuid,
    attachment_id: Uuid,
    host_id: Uuid,
    host_name: Hostname,
    network_id: Uuid,
    network_cidr: CidrValue,
    community_id: Uuid,
    community_name: CommunityName,
    policy_name: NetworkPolicyName,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttachmentCommunityAssignment {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        attachment_id: Uuid,
        host_id: Uuid,
        host_name: Hostname,
        network_id: Uuid,
        network_cidr: CidrValue,
        community_id: Uuid,
        community_name: CommunityName,
        policy_name: NetworkPolicyName,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            attachment_id,
            host_id,
            host_name,
            network_id,
            network_cidr,
            community_id,
            community_name,
            policy_name,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }

    pub fn network_id(&self) -> Uuid {
        self.network_id
    }

    pub fn network_cidr(&self) -> &CidrValue {
        &self.network_cidr
    }

    pub fn community_id(&self) -> Uuid {
        self.community_id
    }

    pub fn community_name(&self) -> &CommunityName {
        &self.community_name
    }

    pub fn policy_name(&self) -> &NetworkPolicyName {
        &self.policy_name
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateAttachmentCommunityAssignment {
    attachment_id: Uuid,
    policy_name: NetworkPolicyName,
    community_name: CommunityName,
}

impl CreateAttachmentCommunityAssignment {
    pub fn new(
        attachment_id: Uuid,
        policy_name: NetworkPolicyName,
        community_name: CommunityName,
    ) -> Self {
        Self {
            attachment_id,
            policy_name,
            community_name,
        }
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn policy_name(&self) -> &NetworkPolicyName {
        &self.policy_name
    }

    pub fn community_name(&self) -> &CommunityName {
        &self.community_name
    }
}

fn normalize_identifier_value(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let normalized = text.trim().to_string();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    })
}

fn normalize_required_text(value: String, label: &str) -> Result<String, AppError> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() {
        return Err(AppError::validation(format!("{label} cannot be empty")));
    }
    Ok(normalized)
}
