use uuid::Uuid;

use crate::domain::{
    attachment::{
        AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
        HostAttachment,
    },
    host::{Host, IpAddressAssignment},
};

#[derive(Clone, Debug)]
pub struct HostView {
    pub host: Host,
    pub attachments: Vec<HostAttachmentView>,
    pub inventory: HostInventoryView,
    pub dns_records: Vec<HostDnsRecordView>,
    pub host_policy: HostPolicyView,
}

impl HostView {
    pub fn new(host: Host) -> Self {
        Self {
            host,
            attachments: Vec::new(),
            inventory: HostInventoryView::default(),
            dns_records: Vec::new(),
            host_policy: HostPolicyView::default(),
        }
    }

    pub fn id(&self) -> Uuid {
        self.host.id()
    }
}

#[derive(Clone, Debug)]
pub struct HostAttachmentView {
    pub attachment: HostAttachment,
    pub ip_addresses: Vec<IpAddressAssignment>,
    pub dhcp_identifiers: Vec<AttachmentDhcpIdentifier>,
    pub prefix_reservations: Vec<AttachmentPrefixReservation>,
    pub community_assignments: Vec<AttachmentCommunityAssignment>,
}

#[derive(Clone, Debug, Default)]
pub struct HostInventoryView {
    pub contacts: Vec<String>,
    pub groups: Vec<String>,
    pub bacnet_id: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct HostDnsRecordView {
    pub id: Uuid,
    pub type_name: String,
    pub ttl: Option<u32>,
    pub rendered: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct HostPolicyView {
    pub roles: Vec<String>,
    pub atoms: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostViewExpansions {
    pub attachments: bool,
    pub inventory: bool,
    pub dns_records: bool,
    pub host_policy: bool,
}

impl HostViewExpansions {
    pub fn summary() -> Self {
        Self::default()
    }

    pub fn detail() -> Self {
        Self {
            attachments: true,
            inventory: true,
            dns_records: true,
            host_policy: true,
        }
    }
}
