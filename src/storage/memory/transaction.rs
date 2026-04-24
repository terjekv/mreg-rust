use std::cell::RefCell;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent},
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, HostAttachment,
            UpdateHostAttachment,
        },
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        community::{Community, CreateCommunity},
        filters::{
            AttachmentCommunityAssignmentFilter, BacnetIdFilter, CommunityFilter,
            HostCommunityAssignmentFilter, HostContactFilter, HostFilter, HostGroupFilter,
            NetworkFilter, NetworkPolicyFilter, PtrOverrideFilter, RecordFilter,
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
    storage::{
        ErasedTxWork, TransactionRunner, TxAttachmentCommunityAssignmentStore, TxAttachmentStore,
        TxAuditStore, TxBacnetStore, TxCommunityStore, TxHostCommunityAssignmentStore,
        TxHostContactStore, TxHostGroupStore, TxHostPolicyStore, TxHostStore, TxLabelStore,
        TxNameServerStore, TxNetworkPolicyStore, TxNetworkStore, TxPtrOverrideStore, TxRecordStore,
        TxStorage, TxZoneStore,
    },
};

use super::{
    MemoryState, MemoryStorage,
    attachments::{
        create_attachment_community_assignment_in_state, create_attachment_dhcp_identifier_in_state,
        create_attachment_in_state, create_attachment_prefix_reservation_in_state,
        delete_attachment_community_assignment_in_state, delete_attachment_dhcp_identifier_in_state,
        delete_attachment_in_state, delete_attachment_prefix_reservation_in_state,
        get_attachment_community_assignment_in_state, get_attachment_in_state,
        list_attachment_community_assignments_for_attachments_in_state,
        list_attachment_community_assignments_in_state,
        list_attachment_dhcp_identifiers_for_attachments_in_state,
        list_attachment_dhcp_identifiers_in_state,
        list_attachment_prefix_reservations_for_attachments_in_state,
        list_attachment_prefix_reservations_in_state, list_attachments_for_host_in_state,
        list_attachments_for_hosts_in_state, list_attachments_for_network_in_state,
        list_attachments_in_state, update_attachment_in_state,
    },
    audit::{list_events_in_state, record_event_in_state},
    bacnet::{
        create_bacnet_id_in_state, delete_bacnet_id_in_state, get_bacnet_id_in_state,
        list_bacnet_ids_for_hosts_in_state, list_bacnet_ids_in_state,
    },
    communities::{
        create_community_in_state, delete_community_in_state, find_community_by_names_in_state,
        get_community_in_state, list_communities_in_state,
    },
    host_community_assignments::{
        create_host_community_assignment_in_state, delete_host_community_assignment_in_state,
        get_host_community_assignment_in_state, list_host_community_assignments_in_state,
    },
    host_contacts::{
        create_host_contact_in_state, delete_host_contact_in_state,
        get_host_contact_by_email_in_state, list_host_contacts_for_hosts_in_state,
        list_host_contacts_in_state,
    },
    host_groups::{
        create_host_group_in_state, delete_host_group_in_state, get_host_group_by_name_in_state,
        list_host_groups_for_hosts_in_state, list_host_groups_in_state,
    },
    host_policy::{
        add_atom_to_role_in_state, add_host_to_role_in_state, add_label_to_role_in_state,
        create_atom_in_state, create_role_in_state, delete_atom_in_state, delete_role_in_state,
        get_atom_by_name_in_state, get_role_by_name_in_state, list_atoms_in_state,
        list_roles_for_host_in_state, list_roles_for_hosts_in_state, list_roles_in_state,
        remove_atom_from_role_in_state, remove_host_from_role_in_state,
        remove_label_from_role_in_state, update_atom_in_state, update_role_in_state,
    },
    hosts::{
        assign_ip_address_in_state, create_host_in_state, delete_host_in_state,
        get_host_auth_context_in_state, get_host_by_name_in_state, get_ip_address_in_state,
        list_hosts_by_names_in_state, list_hosts_in_state,
        list_ip_addresses_for_host_in_state, list_ip_addresses_for_hosts_in_state,
        list_ip_addresses_in_state, unassign_ip_address_in_state, update_host_in_state,
        update_ip_address_in_state,
    },
    labels::{
        create_label_in_state, delete_label_in_state, get_label_by_name_in_state,
        list_labels_in_state, update_label_in_state,
    },
    nameservers::{
        create_nameserver_in_state, delete_nameserver_in_state, get_nameserver_by_name_in_state,
        list_nameservers_in_state, update_nameserver_in_state,
    },
    network_policies::{
        create_network_policy_in_state, delete_network_policy_in_state,
        get_network_policy_by_name_in_state, list_network_policies_in_state,
    },
    networks::{
        add_excluded_range_in_state, count_unused_addresses_in_state, create_network_in_state,
        delete_network_in_state, get_network_by_cidr_in_state, list_excluded_ranges_in_state,
        list_networks_in_state, list_unused_addresses_in_state, list_used_addresses_in_state,
        update_network_in_state,
    },
    ptr_overrides::{
        create_ptr_override_in_state, delete_ptr_override_in_state,
        get_ptr_override_by_address_in_state, list_ptr_overrides_in_state,
    },
    records::{
        create_record_type_in_state, create_record_with_serial_bump_in_state,
        delete_record_in_state, delete_record_type_in_state, delete_records_by_owner_in_state,
        delete_records_by_owner_name_and_type_in_state, delete_rrset_in_state,
        find_records_by_owner_in_state, get_record_in_state, get_rrset_in_state,
        list_record_types_in_state, list_records_for_hosts_in_state, list_records_in_state,
        list_rrsets_in_state, rename_record_owner_in_state, update_record_in_state,
    },
    zones::{
        bump_forward_zone_serial_in_state, bump_reverse_zone_serial_in_state,
        create_forward_zone_delegation_in_state, create_forward_zone_in_state,
        create_reverse_zone_delegation_in_state, create_reverse_zone_in_state,
        delete_forward_zone_delegation_in_state, delete_forward_zone_in_state,
        delete_reverse_zone_delegation_in_state, delete_reverse_zone_in_state,
        get_forward_zone_by_name_in_state, get_reverse_zone_by_name_in_state,
        list_forward_zone_delegations_in_state, list_forward_zones_in_state,
        list_reverse_zone_delegations_in_state, list_reverse_zones_in_state,
        update_forward_zone_in_state, update_reverse_zone_in_state,
    },
};

/// Transaction-scoped view of `MemoryStorage`. Wraps a borrow of a snapshot
/// `MemoryState` taken at the start of the transaction. On commit the runner
/// swaps the snapshot back into the lock; on abort it is dropped.
pub(super) struct MemTxStorage<'tx> {
    state: RefCell<&'tx mut MemoryState>,
}

impl<'tx> MemTxStorage<'tx> {
    fn new(state: &'tx mut MemoryState) -> Self {
        Self {
            state: RefCell::new(state),
        }
    }
}

impl<'tx> TxStorage for MemTxStorage<'tx> {
    fn labels(&self) -> &dyn TxLabelStore {
        self
    }
    fn nameservers(&self) -> &dyn TxNameServerStore {
        self
    }
    fn zones(&self) -> &dyn TxZoneStore {
        self
    }
    fn networks(&self) -> &dyn TxNetworkStore {
        self
    }
    fn hosts(&self) -> &dyn TxHostStore {
        self
    }
    fn attachments(&self) -> &dyn TxAttachmentStore {
        self
    }
    fn attachment_community_assignments(&self) -> &dyn TxAttachmentCommunityAssignmentStore {
        self
    }
    fn host_contacts(&self) -> &dyn TxHostContactStore {
        self
    }
    fn host_groups(&self) -> &dyn TxHostGroupStore {
        self
    }
    fn bacnet(&self) -> &dyn TxBacnetStore {
        self
    }
    fn ptr_overrides(&self) -> &dyn TxPtrOverrideStore {
        self
    }
    fn network_policies(&self) -> &dyn TxNetworkPolicyStore {
        self
    }
    fn communities(&self) -> &dyn TxCommunityStore {
        self
    }
    fn host_community_assignments(&self) -> &dyn TxHostCommunityAssignmentStore {
        self
    }
    fn host_policy(&self) -> &dyn TxHostPolicyStore {
        self
    }
    fn records(&self) -> &dyn TxRecordStore {
        self
    }
    fn audit(&self) -> &dyn TxAuditStore {
        self
    }
}

impl<'tx> TxHostStore for MemTxStorage<'tx> {
    fn list_hosts(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError> {
        list_hosts_in_state(&self.state.borrow(), page, filter)
    }

    fn create_host(&self, command: CreateHost) -> Result<Host, AppError> {
        create_host_in_state(&mut self.state.borrow_mut(), command)
    }

    fn get_host_by_name(&self, name: &Hostname) -> Result<Host, AppError> {
        get_host_by_name_in_state(&self.state.borrow(), name)
    }

    fn list_hosts_by_names(&self, names: &[Hostname]) -> Result<Vec<Host>, AppError> {
        list_hosts_by_names_in_state(&self.state.borrow(), names)
    }

    fn get_host_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError> {
        get_host_auth_context_in_state(&self.state.borrow(), name)
    }

    fn update_host(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError> {
        update_host_in_state(&mut self.state.borrow_mut(), name, command)
    }

    fn delete_host(&self, name: &Hostname) -> Result<(), AppError> {
        delete_host_in_state(&mut self.state.borrow_mut(), name)
    }

    fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        list_ip_addresses_in_state(&self.state.borrow(), page)
    }

    fn list_ip_addresses_for_host(
        &self,
        host: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        list_ip_addresses_for_host_in_state(&self.state.borrow(), host, page)
    }

    fn list_ip_addresses_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        list_ip_addresses_for_hosts_in_state(&self.state.borrow(), hosts)
    }

    fn get_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        get_ip_address_in_state(&self.state.borrow(), address)
    }

    fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        assign_ip_address_in_state(&mut self.state.borrow_mut(), command)
    }

    fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        update_ip_address_in_state(&mut self.state.borrow_mut(), address, command)
    }

    fn unassign_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        unassign_ip_address_in_state(&mut self.state.borrow_mut(), address)
    }
}

impl<'tx> TxAuditStore for MemTxStorage<'tx> {
    fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        Ok(record_event_in_state(&mut self.state.borrow_mut(), event))
    }

    fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        list_events_in_state(&self.state.borrow(), page)
    }
}

impl<'tx> TxLabelStore for MemTxStorage<'tx> {
    fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        list_labels_in_state(&self.state.borrow(), page)
    }
    fn create_label(&self, command: CreateLabel) -> Result<Label, AppError> {
        create_label_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError> {
        get_label_by_name_in_state(&self.state.borrow(), name)
    }
    fn update_label(&self, name: &LabelName, command: UpdateLabel) -> Result<Label, AppError> {
        update_label_in_state(&mut self.state.borrow_mut(), name, command)
    }
    fn delete_label(&self, name: &LabelName) -> Result<(), AppError> {
        delete_label_in_state(&mut self.state.borrow_mut(), name)
    }
}

impl<'tx> TxNameServerStore for MemTxStorage<'tx> {
    fn list_nameservers(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError> {
        list_nameservers_in_state(&self.state.borrow(), page)
    }
    fn create_nameserver(&self, command: CreateNameServer) -> Result<NameServer, AppError> {
        create_nameserver_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_nameserver_by_name(&self, name: &DnsName) -> Result<NameServer, AppError> {
        get_nameserver_by_name_in_state(&self.state.borrow(), name)
    }
    fn update_nameserver(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        update_nameserver_in_state(&mut self.state.borrow_mut(), name, command)
    }
    fn delete_nameserver(&self, name: &DnsName) -> Result<(), AppError> {
        delete_nameserver_in_state(&mut self.state.borrow_mut(), name)
    }
}

impl<'tx> TxZoneStore for MemTxStorage<'tx> {
    fn list_forward_zones(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError> {
        list_forward_zones_in_state(&self.state.borrow(), page)
    }
    fn create_forward_zone(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        create_forward_zone_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_forward_zone_by_name(&self, name: &ZoneName) -> Result<ForwardZone, AppError> {
        get_forward_zone_by_name_in_state(&self.state.borrow(), name)
    }
    fn update_forward_zone(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        update_forward_zone_in_state(&mut self.state.borrow_mut(), name, command)
    }
    fn delete_forward_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        delete_forward_zone_in_state(&mut self.state.borrow_mut(), name)
    }
    fn list_reverse_zones(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError> {
        list_reverse_zones_in_state(&self.state.borrow(), page)
    }
    fn create_reverse_zone(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        create_reverse_zone_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_reverse_zone_by_name(&self, name: &ZoneName) -> Result<ReverseZone, AppError> {
        get_reverse_zone_by_name_in_state(&self.state.borrow(), name)
    }
    fn update_reverse_zone(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        update_reverse_zone_in_state(&mut self.state.borrow_mut(), name, command)
    }
    fn delete_reverse_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        delete_reverse_zone_in_state(&mut self.state.borrow_mut(), name)
    }
    fn list_forward_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        list_forward_zone_delegations_in_state(&self.state.borrow(), zone_name, page)
    }
    fn create_forward_zone_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        create_forward_zone_delegation_in_state(&mut self.state.borrow_mut(), command)
    }
    fn delete_forward_zone_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        delete_forward_zone_delegation_in_state(&mut self.state.borrow_mut(), delegation_id)
    }
    fn list_reverse_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        list_reverse_zone_delegations_in_state(&self.state.borrow(), zone_name, page)
    }
    fn create_reverse_zone_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        create_reverse_zone_delegation_in_state(&mut self.state.borrow_mut(), command)
    }
    fn delete_reverse_zone_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        delete_reverse_zone_delegation_in_state(&mut self.state.borrow_mut(), delegation_id)
    }
    fn bump_forward_zone_serial(&self, zone_id: Uuid) -> Result<ForwardZone, AppError> {
        bump_forward_zone_serial_in_state(&mut self.state.borrow_mut(), zone_id)
    }
    fn bump_reverse_zone_serial(&self, zone_id: Uuid) -> Result<ReverseZone, AppError> {
        bump_reverse_zone_serial_in_state(&mut self.state.borrow_mut(), zone_id)
    }
}

impl<'tx> TxHostContactStore for MemTxStorage<'tx> {
    fn list_host_contacts(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError> {
        list_host_contacts_in_state(&self.state.borrow(), page, filter)
    }
    fn create_host_contact(&self, command: CreateHostContact) -> Result<HostContact, AppError> {
        create_host_contact_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_host_contact_by_email(
        &self,
        email: &EmailAddressValue,
    ) -> Result<HostContact, AppError> {
        get_host_contact_by_email_in_state(&self.state.borrow(), email)
    }
    fn list_host_contacts_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostContact>, AppError> {
        list_host_contacts_for_hosts_in_state(&self.state.borrow(), hosts)
    }
    fn delete_host_contact(&self, email: &EmailAddressValue) -> Result<(), AppError> {
        delete_host_contact_in_state(&mut self.state.borrow_mut(), email)
    }
}

impl<'tx> TxNetworkStore for MemTxStorage<'tx> {
    fn list_networks(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        list_networks_in_state(&self.state.borrow(), page, filter)
    }
    fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError> {
        create_network_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_network_by_cidr(&self, cidr: &CidrValue) -> Result<Network, AppError> {
        get_network_by_cidr_in_state(&self.state.borrow(), cidr)
    }
    fn update_network(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        update_network_in_state(&mut self.state.borrow_mut(), cidr, command)
    }
    fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError> {
        delete_network_in_state(&mut self.state.borrow_mut(), cidr)
    }
    fn list_excluded_ranges(
        &self,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        list_excluded_ranges_in_state(&self.state.borrow(), network, page)
    }
    fn add_excluded_range(
        &self,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        add_excluded_range_in_state(&mut self.state.borrow_mut(), network, command)
    }
    fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        list_used_addresses_in_state(&self.state.borrow(), cidr)
    }
    fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        list_unused_addresses_in_state(&self.state.borrow(), cidr, limit)
    }
    fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError> {
        count_unused_addresses_in_state(&self.state.borrow(), cidr)
    }
}

impl<'tx> TxAttachmentStore for MemTxStorage<'tx> {
    fn list_attachments(&self, page: &PageRequest) -> Result<Page<HostAttachment>, AppError> {
        list_attachments_in_state(&self.state.borrow(), page)
    }
    fn list_attachments_for_host(
        &self,
        host: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        list_attachments_for_host_in_state(&self.state.borrow(), host)
    }
    fn list_attachments_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostAttachment>, AppError> {
        list_attachments_for_hosts_in_state(&self.state.borrow(), hosts)
    }
    fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        list_attachments_for_network_in_state(&self.state.borrow(), network)
    }
    fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        create_attachment_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError> {
        get_attachment_in_state(&self.state.borrow(), attachment_id)
    }
    fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        update_attachment_in_state(&mut self.state.borrow_mut(), attachment_id, command)
    }
    fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError> {
        delete_attachment_in_state(&mut self.state.borrow_mut(), attachment_id)
    }
    fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        list_attachment_dhcp_identifiers_in_state(&self.state.borrow(), attachment_id)
    }
    fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        list_attachment_dhcp_identifiers_for_attachments_in_state(
            &self.state.borrow(),
            attachment_ids,
        )
    }
    fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        create_attachment_dhcp_identifier_in_state(&mut self.state.borrow_mut(), command)
    }
    fn delete_attachment_dhcp_identifier(&self, identifier_id: Uuid) -> Result<(), AppError> {
        delete_attachment_dhcp_identifier_in_state(&mut self.state.borrow_mut(), identifier_id)
    }
    fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        list_attachment_prefix_reservations_in_state(&self.state.borrow(), attachment_id)
    }
    fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        list_attachment_prefix_reservations_for_attachments_in_state(
            &self.state.borrow(),
            attachment_ids,
        )
    }
    fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        create_attachment_prefix_reservation_in_state(&mut self.state.borrow_mut(), command)
    }
    fn delete_attachment_prefix_reservation(&self, reservation_id: Uuid) -> Result<(), AppError> {
        delete_attachment_prefix_reservation_in_state(&mut self.state.borrow_mut(), reservation_id)
    }
}

impl<'tx> TxAttachmentCommunityAssignmentStore for MemTxStorage<'tx> {
    fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        list_attachment_community_assignments_in_state(&self.state.borrow(), page, filter)
    }
    fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        list_attachment_community_assignments_for_attachments_in_state(
            &self.state.borrow(),
            attachment_ids,
        )
    }
    fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        create_attachment_community_assignment_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        get_attachment_community_assignment_in_state(&self.state.borrow(), assignment_id)
    }
    fn delete_attachment_community_assignment(&self, assignment_id: Uuid) -> Result<(), AppError> {
        delete_attachment_community_assignment_in_state(&mut self.state.borrow_mut(), assignment_id)
    }
}

impl<'tx> TxHostGroupStore for MemTxStorage<'tx> {
    fn list_host_groups(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError> {
        list_host_groups_in_state(&self.state.borrow(), page, filter)
    }
    fn create_host_group(&self, command: CreateHostGroup) -> Result<HostGroup, AppError> {
        create_host_group_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_host_group_by_name(&self, name: &HostGroupName) -> Result<HostGroup, AppError> {
        get_host_group_by_name_in_state(&self.state.borrow(), name)
    }
    fn list_host_groups_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostGroup>, AppError> {
        list_host_groups_for_hosts_in_state(&self.state.borrow(), hosts)
    }
    fn delete_host_group(&self, name: &HostGroupName) -> Result<(), AppError> {
        delete_host_group_in_state(&mut self.state.borrow_mut(), name)
    }
}

impl<'tx> TxBacnetStore for MemTxStorage<'tx> {
    fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        list_bacnet_ids_in_state(&self.state.borrow(), page, filter)
    }
    fn create_bacnet_id(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError> {
        create_bacnet_id_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_bacnet_id(
        &self,
        bacnet_id: BacnetIdentifier,
    ) -> Result<BacnetIdAssignment, AppError> {
        get_bacnet_id_in_state(&self.state.borrow(), bacnet_id)
    }
    fn list_bacnet_ids_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<BacnetIdAssignment>, AppError> {
        list_bacnet_ids_for_hosts_in_state(&self.state.borrow(), hosts)
    }
    fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        delete_bacnet_id_in_state(&mut self.state.borrow_mut(), bacnet_id)
    }
}

impl<'tx> TxPtrOverrideStore for MemTxStorage<'tx> {
    fn list_ptr_overrides(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError> {
        list_ptr_overrides_in_state(&self.state.borrow(), page, filter)
    }
    fn create_ptr_override(
        &self,
        command: CreatePtrOverride,
    ) -> Result<PtrOverride, AppError> {
        create_ptr_override_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_ptr_override_by_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<PtrOverride, AppError> {
        get_ptr_override_by_address_in_state(&self.state.borrow(), address)
    }
    fn delete_ptr_override(&self, address: &IpAddressValue) -> Result<(), AppError> {
        delete_ptr_override_in_state(&mut self.state.borrow_mut(), address)
    }
}

impl<'tx> TxNetworkPolicyStore for MemTxStorage<'tx> {
    fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        list_network_policies_in_state(&self.state.borrow(), page, filter)
    }
    fn create_network_policy(
        &self,
        command: CreateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError> {
        create_network_policy_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_network_policy_by_name(
        &self,
        name: &NetworkPolicyName,
    ) -> Result<NetworkPolicy, AppError> {
        get_network_policy_by_name_in_state(&self.state.borrow(), name)
    }
    fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        delete_network_policy_in_state(&mut self.state.borrow_mut(), name)
    }
}

impl<'tx> TxCommunityStore for MemTxStorage<'tx> {
    fn list_communities(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError> {
        list_communities_in_state(&self.state.borrow(), page, filter)
    }
    fn create_community(&self, command: CreateCommunity) -> Result<Community, AppError> {
        create_community_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_community(&self, community_id: Uuid) -> Result<Community, AppError> {
        get_community_in_state(&self.state.borrow(), community_id)
    }
    fn delete_community(&self, community_id: Uuid) -> Result<(), AppError> {
        delete_community_in_state(&mut self.state.borrow_mut(), community_id)
    }
    fn find_community_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError> {
        find_community_by_names_in_state(&self.state.borrow(), policy_name, community_name)
    }
}

impl<'tx> TxHostCommunityAssignmentStore for MemTxStorage<'tx> {
    fn list_host_community_assignments(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError> {
        list_host_community_assignments_in_state(&self.state.borrow(), page, filter)
    }
    fn create_host_community_assignment(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError> {
        create_host_community_assignment_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_host_community_assignment(
        &self,
        mapping_id: Uuid,
    ) -> Result<HostCommunityAssignment, AppError> {
        get_host_community_assignment_in_state(&self.state.borrow(), mapping_id)
    }
    fn delete_host_community_assignment(&self, mapping_id: Uuid) -> Result<(), AppError> {
        delete_host_community_assignment_in_state(&mut self.state.borrow_mut(), mapping_id)
    }
}

impl<'tx> TxHostPolicyStore for MemTxStorage<'tx> {
    fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError> {
        list_atoms_in_state(&self.state.borrow(), page)
    }
    fn create_atom(&self, command: CreateHostPolicyAtom) -> Result<HostPolicyAtom, AppError> {
        create_atom_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_atom_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError> {
        get_atom_by_name_in_state(&self.state.borrow(), name)
    }
    fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        update_atom_in_state(&mut self.state.borrow_mut(), name, command)
    }
    fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError> {
        delete_atom_in_state(&mut self.state.borrow_mut(), name)
    }
    fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError> {
        list_roles_in_state(&self.state.borrow(), page)
    }
    fn list_roles_for_host(&self, host_name: &Hostname) -> Result<Vec<HostPolicyRole>, AppError> {
        list_roles_for_host_in_state(&self.state.borrow(), host_name)
    }
    fn list_roles_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        list_roles_for_hosts_in_state(&self.state.borrow(), hosts)
    }
    fn create_role(&self, command: CreateHostPolicyRole) -> Result<HostPolicyRole, AppError> {
        create_role_in_state(&mut self.state.borrow_mut(), command)
    }
    fn get_role_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError> {
        get_role_by_name_in_state(&self.state.borrow(), name)
    }
    fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        update_role_in_state(&mut self.state.borrow_mut(), name, command)
    }
    fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError> {
        delete_role_in_state(&mut self.state.borrow_mut(), name)
    }
    fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        add_atom_to_role_in_state(&mut self.state.borrow_mut(), role_name, atom_name)
    }
    fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        remove_atom_from_role_in_state(&mut self.state.borrow_mut(), role_name, atom_name)
    }
    fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        add_host_to_role_in_state(&mut self.state.borrow_mut(), role_name, host_name)
    }
    fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        remove_host_from_role_in_state(&mut self.state.borrow_mut(), role_name, host_name)
    }
    fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        add_label_to_role_in_state(&mut self.state.borrow_mut(), role_name, label_name)
    }
    fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        remove_label_from_role_in_state(&mut self.state.borrow_mut(), role_name, label_name)
    }
}

impl<'tx> TxRecordStore for MemTxStorage<'tx> {
    fn list_record_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError> {
        list_record_types_in_state(&self.state.borrow(), page)
    }
    fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError> {
        list_rrsets_in_state(&self.state.borrow(), page)
    }
    fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError> {
        list_records_in_state(&self.state.borrow(), page, filter)
    }
    fn get_record(&self, record_id: Uuid) -> Result<RecordInstance, AppError> {
        get_record_in_state(&self.state.borrow(), record_id)
    }
    fn get_rrset(&self, rrset_id: Uuid) -> Result<RecordRrset, AppError> {
        get_rrset_in_state(&self.state.borrow(), rrset_id)
    }
    fn list_records_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<RecordInstance>, AppError> {
        list_records_for_hosts_in_state(&self.state.borrow(), hosts)
    }
    fn create_record_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError> {
        create_record_type_in_state(&mut self.state.borrow_mut(), command)
    }
    fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError> {
        create_record_with_serial_bump_in_state(&mut self.state.borrow_mut(), command)
    }
    fn update_record(
        &self,
        record_id: Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError> {
        update_record_in_state(&mut self.state.borrow_mut(), record_id, command)
    }
    fn delete_record(&self, record_id: Uuid) -> Result<(), AppError> {
        delete_record_in_state(&mut self.state.borrow_mut(), record_id)
    }
    fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError> {
        delete_record_type_in_state(&mut self.state.borrow_mut(), name)
    }
    fn delete_rrset(&self, rrset_id: Uuid) -> Result<(), AppError> {
        delete_rrset_in_state(&mut self.state.borrow_mut(), rrset_id)
    }
    fn find_records_by_owner(
        &self,
        owner_id: Uuid,
    ) -> Result<Vec<RecordInstance>, AppError> {
        find_records_by_owner_in_state(&self.state.borrow(), owner_id)
    }
    fn delete_records_by_owner(&self, owner_id: Uuid) -> Result<u64, AppError> {
        delete_records_by_owner_in_state(&mut self.state.borrow_mut(), owner_id)
    }
    fn delete_records_by_owner_name_and_type(
        &self,
        owner_name: &DnsName,
        type_name: &RecordTypeName,
    ) -> Result<u64, AppError> {
        delete_records_by_owner_name_and_type_in_state(
            &mut self.state.borrow_mut(),
            owner_name,
            type_name,
        )
    }
    fn rename_record_owner(
        &self,
        owner_id: Uuid,
        new_name: &DnsName,
    ) -> Result<u64, AppError> {
        rename_record_owner_in_state(&mut self.state.borrow_mut(), owner_id, new_name)
    }
}

#[async_trait]
impl TransactionRunner for MemoryStorage {
    async fn run_transaction(
        &self,
        work: Box<dyn ErasedTxWork>,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError> {
        // Single-writer: hold the write lock for the closure body. The closure
        // is sync so it cannot re-enter the lock.
        let mut guard = self.state.write().await;
        let mut snapshot: MemoryState = guard.clone();
        let result = {
            let tx = MemTxStorage::new(&mut snapshot);
            work.run(&tx)
        };
        match result {
            Ok(value) => {
                *guard = snapshot;
                Ok(value)
            }
            Err(err) => {
                drop(snapshot);
                Err(err)
            }
        }
    }
}
