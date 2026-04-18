//! Single source of truth for the `HasId` marker trait used by both storage
//! backends to drive cursor-based pagination. Each backend used to maintain its
//! own copy of the trait, the `impl_has_id!` macro, and the registration list
//! — this module replaces both copies so the registration cannot drift.

use uuid::Uuid;

use crate::{
    audit::HistoryEvent,
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            HostAttachment,
        },
        community::Community,
        exports::{ExportRun, ExportTemplate},
        host::{Host, IpAddressAssignment},
        host_community_assignment::HostCommunityAssignment,
        host_contact::HostContact,
        host_group::HostGroup,
        host_policy::{HostPolicyAtom, HostPolicyRole},
        imports::ImportBatchSummary,
        label::Label,
        nameserver::NameServer,
        network::{ExcludedRange, Network},
        network_policy::NetworkPolicy,
        ptr_override::PtrOverride,
        resource_records::{RecordInstance, RecordRrset, RecordTypeDefinition},
        tasks::TaskEnvelope,
        zone::{ForwardZone, ForwardZoneDelegation, ReverseZone, ReverseZoneDelegation},
    },
};

pub(crate) trait HasId {
    fn id(&self) -> Uuid;
}

macro_rules! impl_has_id {
    ($($type:ty),*$(,)?) => {
        $(
            impl HasId for $type {
                fn id(&self) -> Uuid {
                    self.id()
                }
            }
        )*
    };
}

impl_has_id!(
    HostPolicyAtom,
    HostPolicyRole,
    Label,
    NameServer,
    ForwardZone,
    ReverseZone,
    ForwardZoneDelegation,
    ReverseZoneDelegation,
    Network,
    HostAttachment,
    ExcludedRange,
    Host,
    IpAddressAssignment,
    HostContact,
    HostGroup,
    PtrOverride,
    NetworkPolicy,
    Community,
    AttachmentCommunityAssignment,
    HostCommunityAssignment,
    AttachmentDhcpIdentifier,
    AttachmentPrefixReservation,
    TaskEnvelope,
    ImportBatchSummary,
    ExportTemplate,
    ExportRun,
    RecordTypeDefinition,
    RecordRrset,
    RecordInstance,
    HistoryEvent,
);
