pub mod attachments;
pub mod bacnet;
pub mod communities;
pub mod exports;
pub mod host_community_assignments;
pub mod host_contacts;
pub mod host_groups;
pub mod host_policy;
pub mod host_views;
pub mod hosts;
pub mod imports;
pub mod labels;
pub mod nameservers;
pub mod network_policies;
pub mod networks;
pub mod ptr_overrides;
pub mod records;
pub mod tasks;
pub mod zones;

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    audit::HistoryEvent,
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, HostAttachment,
            UpdateHostAttachment,
        },
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        community::{Community, CreateCommunity},
        exports::{CreateExportRun, CreateExportTemplate, ExportRun, ExportTemplate},
        filters::{
            BacnetIdFilter, CommunityFilter, HostCommunityAssignmentFilter, HostContactFilter,
            HostFilter, HostGroupFilter, NetworkFilter, NetworkPolicyFilter, PtrOverrideFilter,
            RecordFilter,
        },
        host::{
            AssignIpAddress, CreateHost, Host, HostAuthContext, IpAddressAssignment, UpdateHost,
            UpdateIpAddress,
        },
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        host_contact::{CreateHostContact, HostContact},
        host_group::{CreateHostGroup, HostGroup},
        host_policy::{
            CreateHostPolicyAtom, CreateHostPolicyRole, HostPolicyAtom, HostPolicyRole,
            UpdateHostPolicyAtom, UpdateHostPolicyRole,
        },
        imports::{CreateImportBatch, ImportBatchSummary},
        label::{CreateLabel, Label, UpdateLabel},
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        network::{CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork},
        network_policy::{CreateNetworkPolicy, NetworkPolicy},
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        resource_records::{
            CreateRecordInstance, CreateRecordTypeDefinition, RecordInstance, RecordRrset,
            RecordTypeDefinition, UpdateRecord,
        },
        tasks::{CreateTask, TaskEnvelope},
        types::{
            BacnetIdentifier, CidrValue, CommunityName, DnsName, EmailAddressValue, HostGroupName,
            HostPolicyName, Hostname, IpAddressValue, LabelName, NetworkPolicyName, RecordTypeName,
            ZoneName,
        },
        zone::{
            CreateForwardZone, CreateForwardZoneDelegation, CreateReverseZone,
            CreateReverseZoneDelegation, ForwardZone, ForwardZoneDelegation, ReverseZone,
            ReverseZoneDelegation, UpdateForwardZone, UpdateReverseZone,
        },
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, DynStorage, ExportStore, ImportStore, TaskStore},
};

// ---------------------------------------------------------------------------
// Services facade
// ---------------------------------------------------------------------------

/// Pre-wired service facade. All mutations go through here,
/// ensuring audit events and domain events are always recorded.
#[derive(Clone)]
pub struct Services {
    storage: DynStorage,
    events: EventSinkClient,
}

impl Services {
    pub fn new(storage: DynStorage, events: EventSinkClient) -> Self {
        Self { storage, events }
    }

    #[doc(hidden)]
    pub fn inner_storage(&self) -> DynStorage {
        self.storage.clone()
    }

    pub fn labels(&self) -> LabelService<'_> {
        LabelService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn nameservers(&self) -> NameServerService<'_> {
        NameServerService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn zones(&self) -> ZoneService<'_> {
        ZoneService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn networks(&self) -> NetworkService<'_> {
        NetworkService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn hosts(&self) -> HostService<'_> {
        HostService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn attachments(&self) -> AttachmentService<'_> {
        AttachmentService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn records(&self) -> RecordService<'_> {
        RecordService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn host_contacts(&self) -> HostContactService<'_> {
        HostContactService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn host_groups(&self) -> HostGroupService<'_> {
        HostGroupService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn bacnet(&self) -> BacnetService<'_> {
        BacnetService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn ptr_overrides(&self) -> PtrOverrideService<'_> {
        PtrOverrideService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn network_policies(&self) -> NetworkPolicyService<'_> {
        NetworkPolicyService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn communities(&self) -> CommunityService<'_> {
        CommunityService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn host_community_assignments(&self) -> HostCommunityAssignmentService<'_> {
        HostCommunityAssignmentService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn host_policy(&self) -> HostPolicyService<'_> {
        HostPolicyService {
            storage: &self.storage,
            events: &self.events,
        }
    }

    pub fn host_views(&self) -> host_views::HostViewService<'_> {
        host_views::HostViewService {
            store: self.storage.host_views(),
        }
    }

    pub fn tasks(&self) -> TaskService<'_> {
        TaskService {
            store: self.storage.tasks(),
        }
    }

    pub fn imports(&self) -> ImportService<'_> {
        ImportService {
            store: self.storage.imports(),
        }
    }

    pub fn exports(&self) -> ExportService<'_> {
        ExportService {
            store: self.storage.exports(),
        }
    }

    pub fn audit(&self) -> AuditService<'_> {
        AuditService {
            store: self.storage.audit(),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-domain service sub-structs
// ---------------------------------------------------------------------------

pub struct LabelService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl LabelService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        labels::list(self.storage.labels(), page).await
    }
    pub async fn create(&self, command: CreateLabel) -> Result<Label, AppError> {
        labels::create(self.storage, command, self.events).await
    }
    pub async fn get(&self, name: &LabelName) -> Result<Label, AppError> {
        labels::get(self.storage.labels(), name).await
    }
    pub async fn update(&self, name: &LabelName, command: UpdateLabel) -> Result<Label, AppError> {
        labels::update(self.storage, name, command, self.events).await
    }
    pub async fn delete(&self, name: &LabelName) -> Result<(), AppError> {
        labels::delete(self.storage, name, self.events).await
    }
}

pub struct NameServerService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl NameServerService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError> {
        nameservers::list(self.storage.nameservers(), page).await
    }
    pub async fn create(&self, command: CreateNameServer) -> Result<NameServer, AppError> {
        nameservers::create(self.storage, command, self.events).await
    }
    pub async fn get(&self, name: &DnsName) -> Result<NameServer, AppError> {
        nameservers::get(self.storage.nameservers(), name).await
    }
    pub async fn update(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        nameservers::update(self.storage, name, command, self.events).await
    }
    pub async fn delete(&self, name: &DnsName) -> Result<(), AppError> {
        nameservers::delete(self.storage, name, self.events).await
    }
}

pub struct ZoneService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl ZoneService<'_> {
    pub async fn list_forward(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError> {
        zones::list_forward(self.storage.zones(), page).await
    }
    pub async fn create_forward(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        zones::create_forward(self.storage, command, self.events).await
    }
    pub async fn get_forward(&self, name: &ZoneName) -> Result<ForwardZone, AppError> {
        zones::get_forward(self.storage.zones(), name).await
    }
    pub async fn update_forward(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        zones::update_forward(self.storage, name, command, self.events).await
    }
    pub async fn delete_forward(&self, name: &ZoneName) -> Result<(), AppError> {
        zones::delete_forward(self.storage, name, self.events).await
    }
    pub async fn list_reverse(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError> {
        zones::list_reverse(self.storage.zones(), page).await
    }
    pub async fn create_reverse(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        zones::create_reverse(self.storage, command, self.events).await
    }
    pub async fn get_reverse(&self, name: &ZoneName) -> Result<ReverseZone, AppError> {
        zones::get_reverse(self.storage.zones(), name).await
    }
    pub async fn update_reverse(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        zones::update_reverse(self.storage, name, command, self.events).await
    }
    pub async fn delete_reverse(&self, name: &ZoneName) -> Result<(), AppError> {
        zones::delete_reverse(self.storage, name, self.events).await
    }
    pub async fn list_forward_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        zones::list_forward_delegations(self.storage.zones(), zone_name, page).await
    }
    pub async fn create_forward_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        zones::create_forward_delegation(self.storage, command, self.events).await
    }
    pub async fn delete_forward_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        zones::delete_forward_delegation(self.storage, delegation_id, self.events).await
    }
    pub async fn list_reverse_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        zones::list_reverse_delegations(self.storage.zones(), zone_name, page).await
    }
    pub async fn create_reverse_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        zones::create_reverse_delegation(self.storage, command, self.events).await
    }
    pub async fn delete_reverse_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        zones::delete_reverse_delegation(self.storage, delegation_id, self.events).await
    }
}

pub struct NetworkService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl NetworkService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        networks::list(self.storage.networks(), page, filter).await
    }
    pub async fn create(&self, command: CreateNetwork) -> Result<Network, AppError> {
        networks::create(self.storage, command, self.events).await
    }
    pub async fn get(&self, cidr: &CidrValue) -> Result<Network, AppError> {
        networks::get(self.storage.networks(), cidr).await
    }
    pub async fn update(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        networks::update(self.storage, cidr, command, self.events).await
    }
    pub async fn delete(&self, cidr: &CidrValue) -> Result<(), AppError> {
        networks::delete(self.storage, cidr, self.events).await
    }
    pub async fn list_excluded_ranges(
        &self,
        cidr: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        networks::list_excluded_ranges(self.storage.networks(), cidr, page).await
    }
    pub async fn add_excluded_range(
        &self,
        cidr: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        networks::add_excluded_range(self.storage, cidr, command, self.events).await
    }
    pub async fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        networks::list_used_addresses(self.storage.networks(), cidr).await
    }
    pub async fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        networks::list_unused_addresses(self.storage.networks(), cidr, limit).await
    }
    pub async fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError> {
        self.storage.networks().count_unused_addresses(cidr).await
    }
}

pub struct HostService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl HostService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError> {
        hosts::list(self.storage.hosts(), page, filter).await
    }
    pub async fn create(&self, command: CreateHost) -> Result<Host, AppError> {
        hosts::create(self.storage, command, self.events).await
    }
    pub async fn get(&self, name: &Hostname) -> Result<Host, AppError> {
        hosts::get(self.storage.hosts(), name).await
    }
    pub async fn get_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError> {
        hosts::get_auth_context(self.storage.hosts(), name).await
    }
    pub async fn update(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError> {
        hosts::update(self.storage, name, command, self.events).await
    }
    pub async fn delete(&self, name: &Hostname) -> Result<(), AppError> {
        hosts::delete(self.storage, name, self.events).await
    }
    pub async fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        hosts::list_ip_addresses(self.storage.hosts(), page).await
    }
    pub async fn list_host_ip_addresses(
        &self,
        name: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        hosts::list_host_ip_addresses(self.storage.hosts(), name, page).await
    }
    pub async fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        hosts::assign_ip_address(self.storage, command, self.events).await
    }
    pub async fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        hosts::update_ip_address(self.storage, address, command, self.events).await
    }
    pub async fn unassign_ip_address(&self, address: &IpAddressValue) -> Result<(), AppError> {
        hosts::unassign_ip_address(self.storage, address, self.events).await
    }
}

pub struct AttachmentService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl AttachmentService<'_> {
    pub async fn list_attachments_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        self.storage
            .attachments()
            .list_attachments_for_host(host_name)
            .await
    }
    pub async fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        self.storage
            .attachments()
            .list_attachments_for_network(network)
            .await
    }
    pub async fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError> {
        self.storage.attachments().get_attachment(attachment_id).await
    }
    pub async fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        attachments::create_attachment(self.storage, command, self.events).await
    }
    pub async fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        attachments::update_attachment(self.storage, attachment_id, command, self.events).await
    }
    pub async fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError> {
        attachments::delete_attachment(self.storage, attachment_id, self.events).await
    }
    pub async fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        attachments::create_attachment_dhcp_identifier(self.storage, command, self.events).await
    }
    pub async fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        self.storage
            .attachments()
            .list_attachment_dhcp_identifiers(attachment_id)
            .await
    }
    pub async fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        self.storage
            .attachments()
            .list_attachment_dhcp_identifiers_for_attachments(attachment_ids)
            .await
    }
    pub async fn delete_attachment_dhcp_identifier(
        &self,
        attachment_id: Uuid,
        identifier_id: Uuid,
    ) -> Result<(), AppError> {
        attachments::delete_attachment_dhcp_identifier(
            self.storage,
            attachment_id,
            identifier_id,
            self.events,
        )
        .await
    }
    pub async fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        attachments::create_attachment_prefix_reservation(self.storage, command, self.events).await
    }
    pub async fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        self.storage
            .attachments()
            .list_attachment_prefix_reservations(attachment_id)
            .await
    }
    pub async fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        self.storage
            .attachments()
            .list_attachment_prefix_reservations_for_attachments(attachment_ids)
            .await
    }
    pub async fn delete_attachment_prefix_reservation(
        &self,
        attachment_id: Uuid,
        reservation_id: Uuid,
    ) -> Result<(), AppError> {
        attachments::delete_attachment_prefix_reservation(
            self.storage,
            attachment_id,
            reservation_id,
            self.events,
        )
        .await
    }
    pub async fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &crate::domain::filters::AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        self.storage
            .attachment_community_assignments()
            .list_attachment_community_assignments(page, filter)
            .await
    }
    pub async fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        self.storage
            .attachment_community_assignments()
            .list_attachment_community_assignments_for_attachments(attachment_ids)
            .await
    }
    pub async fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        attachments::create_attachment_community_assignment(self.storage, command, self.events).await
    }
    pub async fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        self.storage
            .attachment_community_assignments()
            .get_attachment_community_assignment(assignment_id)
            .await
    }
    pub async fn delete_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<(), AppError> {
        attachments::delete_attachment_community_assignment(self.storage, assignment_id, self.events)
            .await
    }
}

pub struct RecordService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl RecordService<'_> {
    pub async fn list_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError> {
        records::list_types(self.storage.records(), page).await
    }
    pub async fn create_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError> {
        records::create_type(self.storage, command, self.events).await
    }
    pub async fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError> {
        records::delete_record_type(self.storage, name, self.events).await
    }
    pub async fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError> {
        records::list_records(self.storage.records(), page, filter).await
    }
    pub async fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError> {
        records::list_rrsets(self.storage.records(), page).await
    }
    pub async fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError> {
        records::create_record(self.storage, command, self.events).await
    }
    pub async fn get_record(&self, record_id: Uuid) -> Result<RecordInstance, AppError> {
        records::get_record(self.storage.records(), record_id).await
    }
    pub async fn get_rrset(&self, rrset_id: Uuid) -> Result<RecordRrset, AppError> {
        records::get_rrset(self.storage.records(), rrset_id).await
    }
    pub async fn update_record(
        &self,
        record_id: Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError> {
        records::update_record(self.storage, record_id, command, self.events).await
    }
    pub async fn delete_record(&self, record_id: Uuid) -> Result<(), AppError> {
        records::delete_record(self.storage, record_id, self.events).await
    }
    pub async fn delete_rrset(&self, rrset_id: Uuid) -> Result<(), AppError> {
        records::delete_rrset(self.storage, rrset_id, self.events).await
    }
}

pub struct HostContactService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl HostContactService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError> {
        host_contacts::list_host_contacts(self.storage.host_contacts(), page, filter).await
    }
    pub async fn create(&self, command: CreateHostContact) -> Result<HostContact, AppError> {
        host_contacts::create_host_contact(self.storage, command, self.events).await
    }
    pub async fn get(&self, email: &EmailAddressValue) -> Result<HostContact, AppError> {
        host_contacts::get_host_contact(self.storage.host_contacts(), email).await
    }
    pub async fn delete(&self, email: &EmailAddressValue) -> Result<(), AppError> {
        host_contacts::delete_host_contact(self.storage, email, self.events).await
    }
}

pub struct HostGroupService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl HostGroupService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError> {
        host_groups::list_host_groups(self.storage.host_groups(), page, filter).await
    }
    pub async fn create(&self, command: CreateHostGroup) -> Result<HostGroup, AppError> {
        host_groups::create_host_group(self.storage, command, self.events).await
    }
    pub async fn get(&self, name: &HostGroupName) -> Result<HostGroup, AppError> {
        host_groups::get_host_group(self.storage.host_groups(), name).await
    }
    pub async fn delete(&self, name: &HostGroupName) -> Result<(), AppError> {
        host_groups::delete_host_group(self.storage, name, self.events).await
    }
}

pub struct BacnetService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl BacnetService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        bacnet::list_bacnet_ids(self.storage.bacnet(), page, filter).await
    }
    pub async fn create(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError> {
        bacnet::create_bacnet_id(self.storage, command, self.events).await
    }
    pub async fn get(&self, bacnet_id: BacnetIdentifier) -> Result<BacnetIdAssignment, AppError> {
        bacnet::get_bacnet_id(self.storage.bacnet(), bacnet_id).await
    }
    pub async fn delete(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        bacnet::delete_bacnet_id(self.storage, bacnet_id, self.events).await
    }
}

pub struct PtrOverrideService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl PtrOverrideService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError> {
        ptr_overrides::list_ptr_overrides(self.storage.ptr_overrides(), page, filter).await
    }
    pub async fn create(&self, command: CreatePtrOverride) -> Result<PtrOverride, AppError> {
        ptr_overrides::create_ptr_override(self.storage, command, self.events).await
    }
    pub async fn get(&self, address: &IpAddressValue) -> Result<PtrOverride, AppError> {
        ptr_overrides::get_ptr_override(self.storage.ptr_overrides(), address).await
    }
    pub async fn delete(&self, address: &IpAddressValue) -> Result<(), AppError> {
        ptr_overrides::delete_ptr_override(self.storage, address, self.events).await
    }
}

pub struct NetworkPolicyService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl NetworkPolicyService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        network_policies::list_network_policies(self.storage.network_policies(), page, filter).await
    }
    pub async fn create(&self, command: CreateNetworkPolicy) -> Result<NetworkPolicy, AppError> {
        network_policies::create_network_policy(self.storage, command, self.events).await
    }
    pub async fn get(&self, name: &NetworkPolicyName) -> Result<NetworkPolicy, AppError> {
        network_policies::get_network_policy(self.storage.network_policies(), name).await
    }
    pub async fn delete(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        network_policies::delete_network_policy(self.storage, name, self.events).await
    }
}

pub struct CommunityService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl CommunityService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError> {
        communities::list_communities(self.storage.communities(), page, filter).await
    }
    pub async fn create(&self, command: CreateCommunity) -> Result<Community, AppError> {
        communities::create_community(self.storage, command, self.events).await
    }
    pub async fn get(&self, community_id: Uuid) -> Result<Community, AppError> {
        communities::get_community(self.storage.communities(), community_id).await
    }
    pub async fn delete(&self, community_id: Uuid) -> Result<(), AppError> {
        communities::delete_community(self.storage, community_id, self.events).await
    }
    pub async fn find_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError> {
        communities::find_community_by_names(self.storage.communities(), policy_name, community_name)
            .await
    }
}

pub struct HostCommunityAssignmentService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl HostCommunityAssignmentService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError> {
        host_community_assignments::list_host_community_assignments(
            self.storage.host_community_assignments(),
            page,
            filter,
        )
        .await
    }
    pub async fn create(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError> {
        host_community_assignments::create_host_community_assignment(
            self.storage,
            command,
            self.events,
        )
        .await
    }
    pub async fn get(&self, mapping_id: Uuid) -> Result<HostCommunityAssignment, AppError> {
        host_community_assignments::get_host_community_assignment(
            self.storage.host_community_assignments(),
            mapping_id,
        )
        .await
    }
    pub async fn delete(&self, mapping_id: Uuid) -> Result<(), AppError> {
        host_community_assignments::delete_host_community_assignment(
            self.storage,
            mapping_id,
            self.events,
        )
        .await
    }
}

pub struct HostPolicyService<'a> {
    storage: &'a DynStorage,
    events: &'a EventSinkClient,
}

impl HostPolicyService<'_> {
    pub async fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError> {
        host_policy::list_atoms(self.storage.host_policy(), page).await
    }
    pub async fn create_atom(
        &self,
        command: CreateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        host_policy::create_atom(self.storage, command, self.events).await
    }
    pub async fn get_atom(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError> {
        host_policy::get_atom(self.storage.host_policy(), name).await
    }
    pub async fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        host_policy::update_atom(self.storage, name, command, self.events).await
    }
    pub async fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError> {
        host_policy::delete_atom(self.storage, name, self.events).await
    }
    pub async fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError> {
        host_policy::list_roles(self.storage.host_policy(), page).await
    }
    pub async fn list_roles_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        self.storage.host_policy().list_roles_for_host(host_name).await
    }
    pub async fn create_role(
        &self,
        command: CreateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        host_policy::create_role(self.storage, command, self.events).await
    }
    pub async fn get_role(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError> {
        host_policy::get_role(self.storage.host_policy(), name).await
    }
    pub async fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        host_policy::update_role(self.storage, name, command, self.events).await
    }
    pub async fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError> {
        host_policy::delete_role(self.storage, name, self.events).await
    }
    pub async fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        host_policy::add_atom_to_role(self.storage, role_name, atom_name, self.events).await
    }
    pub async fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        host_policy::remove_atom_from_role(self.storage, role_name, atom_name, self.events).await
    }
    pub async fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        host_policy::add_host_to_role(self.storage, role_name, host_name, self.events).await
    }
    pub async fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        host_policy::remove_host_from_role(self.storage, role_name, host_name, self.events).await
    }
    pub async fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        host_policy::add_label_to_role(self.storage, role_name, label_name, self.events).await
    }
    pub async fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        host_policy::remove_label_from_role(self.storage, role_name, label_name, self.events).await
    }
}

pub struct TaskService<'a> {
    store: &'a (dyn TaskStore + Send + Sync),
}

impl TaskService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<TaskEnvelope>, AppError> {
        tasks::list(self.store, page).await
    }
    pub async fn create(&self, command: CreateTask) -> Result<TaskEnvelope, AppError> {
        tasks::create(self.store, command).await
    }
    pub async fn claim_next(&self) -> Result<Option<TaskEnvelope>, AppError> {
        tasks::claim_next(self.store).await
    }
    pub async fn complete(&self, task_id: Uuid, result: Value) -> Result<TaskEnvelope, AppError> {
        tasks::complete(self.store, task_id, result).await
    }
    pub async fn fail(
        &self,
        task_id: Uuid,
        error_summary: String,
    ) -> Result<TaskEnvelope, AppError> {
        tasks::fail(self.store, task_id, error_summary).await
    }
    pub async fn cancel(&self, task_id: Uuid) -> Result<TaskEnvelope, AppError> {
        tasks::cancel(self.store, task_id).await
    }
    pub async fn purge_finished_before(&self, cutoff: DateTime<Utc>) -> Result<usize, AppError> {
        tasks::purge_finished_before(self.store, cutoff).await
    }
}

pub struct ImportService<'a> {
    store: &'a (dyn ImportStore + Send + Sync),
}

impl ImportService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<ImportBatchSummary>, AppError> {
        imports::list(self.store, page).await
    }
    pub async fn create(&self, command: CreateImportBatch) -> Result<ImportBatchSummary, AppError> {
        imports::create(self.store, command).await
    }
    pub async fn run(&self, import_id: Uuid) -> Result<ImportBatchSummary, AppError> {
        imports::run(self.store, import_id).await
    }
}

pub struct ExportService<'a> {
    store: &'a (dyn ExportStore + Send + Sync),
}

impl ExportService<'_> {
    pub async fn list_templates(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ExportTemplate>, AppError> {
        exports::list_templates(self.store, page).await
    }
    pub async fn create_template(
        &self,
        command: CreateExportTemplate,
    ) -> Result<ExportTemplate, AppError> {
        exports::create_template(self.store, command).await
    }
    pub async fn list_runs(&self, page: &PageRequest) -> Result<Page<ExportRun>, AppError> {
        exports::list_runs(self.store, page).await
    }
    pub async fn create_run(&self, command: CreateExportRun) -> Result<ExportRun, AppError> {
        exports::create_run(self.store, command).await
    }
    pub async fn run_export(&self, run_id: Uuid) -> Result<ExportRun, AppError> {
        exports::run_export(self.store, run_id).await
    }
}

pub struct AuditService<'a> {
    store: &'a (dyn AuditStore + Send + Sync),
}

impl AuditService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        self.store.list_events(page).await
    }
}
