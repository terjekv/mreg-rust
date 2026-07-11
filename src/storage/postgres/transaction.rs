use std::cell::RefCell;

use async_trait::async_trait;
use diesel::{Connection, PgConnection};
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

use super::{PostgresStorage, audit, helpers::vec_to_page, host_contacts as pg_host_contacts};

/// Transaction-scoped view of `PostgresStorage`. The connection lives only
/// for the duration of the closure passed to
/// [`crate::storage::DynStorage::transaction`]; sub-store methods take `&self`
/// and borrow it through a `RefCell`.
///
/// Sound because the closure runs single-threaded inside one
/// `spawn_blocking` worker (Diesel + r2d2 connections are `!Sync`).
pub(super) struct PgTxStorage<'c> {
    conn: RefCell<&'c mut PgConnection>,
}

impl<'c> PgTxStorage<'c> {
    fn new(conn: &'c mut PgConnection) -> Self {
        Self {
            conn: RefCell::new(conn),
        }
    }
}

impl<'c> TxStorage for PgTxStorage<'c> {
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

impl<'c> TxHostStore for PgTxStorage<'c> {
    fn list_hosts(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError> {
        PostgresStorage::list_hosts_in_conn(&mut self.conn.borrow_mut(), page, filter)
    }

    fn create_host(&self, command: CreateHost) -> Result<Host, AppError> {
        PostgresStorage::create_host_in_conn(&mut self.conn.borrow_mut(), command)
    }

    fn get_host_by_name(&self, name: &Hostname) -> Result<Host, AppError> {
        PostgresStorage::query_host_by_name(&mut self.conn.borrow_mut(), name)
    }

    fn list_hosts_by_names(&self, names: &[Hostname]) -> Result<Vec<Host>, AppError> {
        let mut conn = self.conn.borrow_mut();
        let ids = PostgresStorage::resolve_host_ids(&mut conn, names)?;
        let mut hosts = PostgresStorage::query_hosts(&mut conn)?
            .into_iter()
            .filter(|host| ids.contains_key(host.name()))
            .collect::<Vec<_>>();
        hosts.sort_by_key(|host| host.name().as_str().to_string());
        Ok(hosts)
    }

    fn get_host_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError> {
        PostgresStorage::query_host_auth_context(&mut self.conn.borrow_mut(), name)
    }

    fn update_host(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError> {
        PostgresStorage::update_host_in_conn(&mut self.conn.borrow_mut(), name, command)
    }

    fn delete_host(&self, name: &Hostname) -> Result<(), AppError> {
        PostgresStorage::delete_host_in_conn(&mut self.conn.borrow_mut(), name)
    }

    fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        let items = PostgresStorage::query_ip_addresses(&mut self.conn.borrow_mut())?;
        Ok(vec_to_page(items, page))
    }

    fn list_ip_addresses_for_host(
        &self,
        host: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        let items =
            PostgresStorage::query_ip_addresses_for_host(&mut self.conn.borrow_mut(), host)?;
        Ok(vec_to_page(items, page))
    }

    fn list_ip_addresses_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        let mut conn = self.conn.borrow_mut();
        let host_ids = PostgresStorage::resolve_host_ids(&mut conn, hosts)?;
        let ids = host_ids.into_values().collect::<Vec<_>>();
        PostgresStorage::query_ip_addresses_for_hosts(&mut conn, &ids)
    }

    fn get_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        PostgresStorage::query_ip_address(&mut self.conn.borrow_mut(), address)
    }

    fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        PostgresStorage::assign_ip_address_in_conn(&mut self.conn.borrow_mut(), command)
    }

    fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        PostgresStorage::update_ip_address_in_conn(
            &mut self.conn.borrow_mut(),
            address,
            command,
        )
    }

    fn unassign_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        PostgresStorage::unassign_ip_address_in_conn(&mut self.conn.borrow_mut(), address)
    }
}

impl<'c> TxAuditStore for PgTxStorage<'c> {
    fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        audit::record_event_in_conn(&mut self.conn.borrow_mut(), event)
    }

    fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        audit::list_events_in_conn(&mut self.conn.borrow_mut(), page)
    }
}

impl<'c> TxLabelStore for PgTxStorage<'c> {
    fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        PostgresStorage::list_labels_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn create_label(&self, command: CreateLabel) -> Result<Label, AppError> {
        PostgresStorage::create_label_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError> {
        PostgresStorage::get_label_by_name_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn update_label(&self, name: &LabelName, command: UpdateLabel) -> Result<Label, AppError> {
        PostgresStorage::update_label_in_conn(&mut self.conn.borrow_mut(), name, command)
    }
    fn delete_label(&self, name: &LabelName) -> Result<(), AppError> {
        PostgresStorage::delete_label_in_conn(&mut self.conn.borrow_mut(), name)
    }
}

impl<'c> TxNameServerStore for PgTxStorage<'c> {
    fn list_nameservers(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError> {
        PostgresStorage::list_nameservers_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn create_nameserver(&self, command: CreateNameServer) -> Result<NameServer, AppError> {
        PostgresStorage::create_nameserver_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn get_nameserver_by_name(&self, name: &DnsName) -> Result<NameServer, AppError> {
        PostgresStorage::get_nameserver_by_name_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn update_nameserver(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        PostgresStorage::update_nameserver_in_conn(&mut self.conn.borrow_mut(), name, command)
    }
    fn delete_nameserver(&self, name: &DnsName) -> Result<(), AppError> {
        PostgresStorage::delete_nameserver_in_conn(&mut self.conn.borrow_mut(), name)
    }
}

impl<'c> TxHostContactStore for PgTxStorage<'c> {
    fn list_host_contacts(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError> {
        pg_host_contacts::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_host_contact(&self, command: CreateHostContact) -> Result<HostContact, AppError> {
        pg_host_contacts::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_host_contact_by_email(
        &self,
        email: &EmailAddressValue,
    ) -> Result<HostContact, AppError> {
        pg_host_contacts::get_by_email(&mut self.conn.borrow_mut(), email.as_str())
    }
    fn list_host_contacts_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostContact>, AppError> {
        pg_host_contacts::list_for_hosts(&mut self.conn.borrow_mut(), hosts)
    }
    fn delete_host_contact(&self, email: &EmailAddressValue) -> Result<(), AppError> {
        pg_host_contacts::delete(&mut self.conn.borrow_mut(), email.as_str())
    }
}

impl<'c> TxZoneStore for PgTxStorage<'c> {
    fn list_forward_zones(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError> {
        PostgresStorage::list_forward_zones_impl(&mut self.conn.borrow_mut(), page)
    }
    fn create_forward_zone(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        PostgresStorage::create_forward_zone_impl(&mut self.conn.borrow_mut(), command)
    }
    fn get_forward_zone_by_name(&self, name: &ZoneName) -> Result<ForwardZone, AppError> {
        PostgresStorage::get_forward_zone_by_name_impl(&mut self.conn.borrow_mut(), name.as_str())
    }
    fn update_forward_zone(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        PostgresStorage::update_forward_zone_impl(
            &mut self.conn.borrow_mut(),
            name.as_str(),
            command,
        )
    }
    fn delete_forward_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        PostgresStorage::delete_forward_zone_impl(&mut self.conn.borrow_mut(), name.as_str())
    }
    fn list_reverse_zones(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError> {
        PostgresStorage::list_reverse_zones_impl(&mut self.conn.borrow_mut(), page)
    }
    fn create_reverse_zone(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        PostgresStorage::create_reverse_zone_impl(&mut self.conn.borrow_mut(), command)
    }
    fn get_reverse_zone_by_name(&self, name: &ZoneName) -> Result<ReverseZone, AppError> {
        PostgresStorage::get_reverse_zone_by_name_impl(&mut self.conn.borrow_mut(), name.as_str())
    }
    fn update_reverse_zone(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        PostgresStorage::update_reverse_zone_impl(
            &mut self.conn.borrow_mut(),
            name.as_str(),
            command,
        )
    }
    fn delete_reverse_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        PostgresStorage::delete_reverse_zone_impl(&mut self.conn.borrow_mut(), name.as_str())
    }
    fn list_forward_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        PostgresStorage::list_forward_zone_delegations_impl(
            &mut self.conn.borrow_mut(),
            zone_name.as_str(),
            page,
        )
    }
    fn create_forward_zone_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        PostgresStorage::create_forward_zone_delegation_impl(&mut self.conn.borrow_mut(), command)
    }
    fn delete_forward_zone_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_forward_zone_delegation_impl(
            &mut self.conn.borrow_mut(),
            delegation_id,
        )
    }
    fn list_reverse_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        PostgresStorage::list_reverse_zone_delegations_impl(
            &mut self.conn.borrow_mut(),
            zone_name.as_str(),
            page,
        )
    }
    fn create_reverse_zone_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        PostgresStorage::create_reverse_zone_delegation_impl(&mut self.conn.borrow_mut(), command)
    }
    fn delete_reverse_zone_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_reverse_zone_delegation_impl(
            &mut self.conn.borrow_mut(),
            delegation_id,
        )
    }
    fn bump_forward_zone_serial(&self, zone_id: Uuid) -> Result<ForwardZone, AppError> {
        PostgresStorage::bump_forward_zone_serial_impl(&mut self.conn.borrow_mut(), zone_id)
    }
    fn bump_reverse_zone_serial(&self, zone_id: Uuid) -> Result<ReverseZone, AppError> {
        PostgresStorage::bump_reverse_zone_serial_impl(&mut self.conn.borrow_mut(), zone_id)
    }
}

impl<'c> TxNetworkStore for PgTxStorage<'c> {
    fn list_networks(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        PostgresStorage::list_networks_in_conn(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError> {
        PostgresStorage::create_network_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn get_network_by_cidr(&self, cidr: &CidrValue) -> Result<Network, AppError> {
        PostgresStorage::query_network_by_cidr(&mut self.conn.borrow_mut(), cidr)
    }
    fn update_network(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        PostgresStorage::update_network_in_conn(&mut self.conn.borrow_mut(), cidr, command)
    }
    fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError> {
        PostgresStorage::delete_network_in_conn(&mut self.conn.borrow_mut(), cidr)
    }
    fn list_excluded_ranges(
        &self,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        PostgresStorage::list_excluded_ranges_in_conn(
            &mut self.conn.borrow_mut(),
            network,
            page,
        )
    }
    fn add_excluded_range(
        &self,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        PostgresStorage::add_excluded_range_in_conn(
            &mut self.conn.borrow_mut(),
            network,
            command,
        )
    }
    fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        PostgresStorage::list_used_addresses_in_conn(&mut self.conn.borrow_mut(), cidr)
    }
    fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        PostgresStorage::list_unused_addresses_in_conn(&mut self.conn.borrow_mut(), cidr, limit)
    }
    fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError> {
        PostgresStorage::count_unused_addresses_in_conn(&mut self.conn.borrow_mut(), cidr)
    }
}

impl<'c> TxAttachmentStore for PgTxStorage<'c> {
    fn list_attachments(&self, page: &PageRequest) -> Result<Page<HostAttachment>, AppError> {
        PostgresStorage::list_attachments_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn list_attachments_for_host(
        &self,
        host: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        PostgresStorage::list_attachments_for_host_in_conn(&mut self.conn.borrow_mut(), host)
    }
    fn list_attachments_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostAttachment>, AppError> {
        PostgresStorage::list_attachments_for_hosts_in_conn(&mut self.conn.borrow_mut(), hosts)
    }
    fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        PostgresStorage::list_attachments_for_network_in_conn(
            &mut self.conn.borrow_mut(),
            network,
        )
    }
    fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        PostgresStorage::create_attachment_tx(&mut self.conn.borrow_mut(), command)
    }
    fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError> {
        PostgresStorage::query_attachment_by_id(&mut self.conn.borrow_mut(), attachment_id)
    }
    fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        PostgresStorage::update_attachment_tx(
            &mut self.conn.borrow_mut(),
            attachment_id,
            command,
        )
    }
    fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_attachment_in_conn(&mut self.conn.borrow_mut(), attachment_id)
    }
    fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        PostgresStorage::list_attachment_dhcp_identifiers_tx(
            &mut self.conn.borrow_mut(),
            attachment_id,
        )
    }
    fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        PostgresStorage::list_attachment_dhcp_identifiers_for_attachments_tx(
            &mut self.conn.borrow_mut(),
            attachment_ids,
        )
    }
    fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        PostgresStorage::create_attachment_dhcp_identifier_tx(
            &mut self.conn.borrow_mut(),
            command,
        )
    }
    fn delete_attachment_dhcp_identifier(&self, identifier_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_attachment_dhcp_identifier_in_conn(
            &mut self.conn.borrow_mut(),
            identifier_id,
        )
    }
    fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        PostgresStorage::list_attachment_prefix_reservations_tx(
            &mut self.conn.borrow_mut(),
            attachment_id,
        )
    }
    fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        PostgresStorage::list_attachment_prefix_reservations_for_attachments_tx(
            &mut self.conn.borrow_mut(),
            attachment_ids,
        )
    }
    fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        PostgresStorage::create_attachment_prefix_reservation_tx(
            &mut self.conn.borrow_mut(),
            command,
        )
    }
    fn delete_attachment_prefix_reservation(&self, reservation_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_attachment_prefix_reservation_in_conn(
            &mut self.conn.borrow_mut(),
            reservation_id,
        )
    }
}

impl<'c> TxAttachmentCommunityAssignmentStore for PgTxStorage<'c> {
    fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        PostgresStorage::list_attachment_community_assignments_in_conn(
            &mut self.conn.borrow_mut(),
            page,
            filter,
        )
    }
    fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        PostgresStorage::list_attachment_community_assignments_for_attachments_tx(
            &mut self.conn.borrow_mut(),
            attachment_ids,
        )
    }
    fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        PostgresStorage::create_attachment_community_assignment_tx(
            &mut self.conn.borrow_mut(),
            command,
        )
    }
    fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        PostgresStorage::get_attachment_community_assignment_in_conn(
            &mut self.conn.borrow_mut(),
            assignment_id,
        )
    }
    fn delete_attachment_community_assignment(&self, assignment_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_attachment_community_assignment_in_conn(
            &mut self.conn.borrow_mut(),
            assignment_id,
        )
    }
}

impl<'c> TxHostGroupStore for PgTxStorage<'c> {
    fn list_host_groups(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError> {
        super::host_groups::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_host_group(&self, command: CreateHostGroup) -> Result<HostGroup, AppError> {
        super::host_groups::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_host_group_by_name(&self, name: &HostGroupName) -> Result<HostGroup, AppError> {
        super::host_groups::get_by_name(&mut self.conn.borrow_mut(), name.as_str())
    }
    fn list_host_groups_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostGroup>, AppError> {
        super::host_groups::list_for_hosts(&mut self.conn.borrow_mut(), hosts)
    }
    fn delete_host_group(&self, name: &HostGroupName) -> Result<(), AppError> {
        super::host_groups::delete(&mut self.conn.borrow_mut(), name.as_str())
    }
}

impl<'c> TxBacnetStore for PgTxStorage<'c> {
    fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        super::bacnet_ids::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_bacnet_id(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError> {
        super::bacnet_ids::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_bacnet_id(
        &self,
        bacnet_id: BacnetIdentifier,
    ) -> Result<BacnetIdAssignment, AppError> {
        super::bacnet_ids::get(&mut self.conn.borrow_mut(), bacnet_id)
    }
    fn list_bacnet_ids_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<BacnetIdAssignment>, AppError> {
        super::bacnet_ids::list_for_hosts(&mut self.conn.borrow_mut(), hosts)
    }
    fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        super::bacnet_ids::delete(&mut self.conn.borrow_mut(), bacnet_id)
    }
}

impl<'c> TxPtrOverrideStore for PgTxStorage<'c> {
    fn list_ptr_overrides(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError> {
        super::ptr_overrides::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_ptr_override(
        &self,
        command: CreatePtrOverride,
    ) -> Result<PtrOverride, AppError> {
        super::ptr_overrides::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_ptr_override_by_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<PtrOverride, AppError> {
        super::ptr_overrides::get_by_address(&mut self.conn.borrow_mut(), &address.as_str())
    }
    fn delete_ptr_override(&self, address: &IpAddressValue) -> Result<(), AppError> {
        super::ptr_overrides::delete(&mut self.conn.borrow_mut(), &address.as_str())
    }
}

impl<'c> TxNetworkPolicyStore for PgTxStorage<'c> {
    fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        super::network_policies::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_network_policy(
        &self,
        command: CreateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError> {
        super::network_policies::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_network_policy_by_name(
        &self,
        name: &NetworkPolicyName,
    ) -> Result<NetworkPolicy, AppError> {
        super::network_policies::get_by_name(&mut self.conn.borrow_mut(), name.as_str())
    }
    fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        super::network_policies::delete(&mut self.conn.borrow_mut(), name.as_str())
    }
}

impl<'c> TxCommunityStore for PgTxStorage<'c> {
    fn list_communities(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError> {
        super::communities::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_community(&self, command: CreateCommunity) -> Result<Community, AppError> {
        super::communities::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_community(&self, community_id: Uuid) -> Result<Community, AppError> {
        super::communities::get_by_id(&mut self.conn.borrow_mut(), community_id)
    }
    fn delete_community(&self, community_id: Uuid) -> Result<(), AppError> {
        super::communities::delete_by_id(&mut self.conn.borrow_mut(), community_id)
    }
    fn find_community_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError> {
        super::communities::find_by_names(
            &mut self.conn.borrow_mut(),
            policy_name.as_str(),
            community_name.as_str(),
        )
    }
}

impl<'c> TxHostCommunityAssignmentStore for PgTxStorage<'c> {
    fn list_host_community_assignments(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError> {
        super::host_community_assignments::list(&mut self.conn.borrow_mut(), page, filter)
    }
    fn create_host_community_assignment(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError> {
        super::host_community_assignments::create(&mut self.conn.borrow_mut(), command)
    }
    fn get_host_community_assignment(
        &self,
        mapping_id: Uuid,
    ) -> Result<HostCommunityAssignment, AppError> {
        super::host_community_assignments::get_by_id(&mut self.conn.borrow_mut(), mapping_id)
    }
    fn delete_host_community_assignment(&self, mapping_id: Uuid) -> Result<(), AppError> {
        super::host_community_assignments::delete_by_id(&mut self.conn.borrow_mut(), mapping_id)
    }
}

impl<'c> TxHostPolicyStore for PgTxStorage<'c> {
    fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError> {
        PostgresStorage::list_atoms_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn create_atom(&self, command: CreateHostPolicyAtom) -> Result<HostPolicyAtom, AppError> {
        PostgresStorage::create_atom_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn get_atom_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError> {
        PostgresStorage::get_atom_by_name_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        PostgresStorage::update_atom_in_conn(&mut self.conn.borrow_mut(), name, command)
    }
    fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError> {
        PostgresStorage::delete_atom_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError> {
        PostgresStorage::list_roles_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn list_roles_for_host(&self, host_name: &Hostname) -> Result<Vec<HostPolicyRole>, AppError> {
        PostgresStorage::list_roles_for_host_in_conn(&mut self.conn.borrow_mut(), host_name)
    }
    fn list_roles_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        PostgresStorage::list_roles_for_hosts_in_conn(&mut self.conn.borrow_mut(), hosts)
    }
    fn create_role(&self, command: CreateHostPolicyRole) -> Result<HostPolicyRole, AppError> {
        PostgresStorage::create_role_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn get_role_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError> {
        PostgresStorage::get_role_by_name_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        PostgresStorage::update_role_in_conn(&mut self.conn.borrow_mut(), name, command)
    }
    fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError> {
        PostgresStorage::delete_role_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        PostgresStorage::add_atom_to_role_in_conn(
            &mut self.conn.borrow_mut(),
            role_name,
            atom_name,
        )
    }
    fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        PostgresStorage::remove_atom_from_role_in_conn(
            &mut self.conn.borrow_mut(),
            role_name,
            atom_name,
        )
    }
    fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        PostgresStorage::add_host_to_role_in_conn(
            &mut self.conn.borrow_mut(),
            role_name,
            host_name,
        )
    }
    fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        PostgresStorage::remove_host_from_role_in_conn(
            &mut self.conn.borrow_mut(),
            role_name,
            host_name,
        )
    }
    fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        PostgresStorage::add_label_to_role_in_conn(
            &mut self.conn.borrow_mut(),
            role_name,
            label_name,
        )
    }
    fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        PostgresStorage::remove_label_from_role_in_conn(
            &mut self.conn.borrow_mut(),
            role_name,
            label_name,
        )
    }
}

impl<'c> TxRecordStore for PgTxStorage<'c> {
    fn list_record_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError> {
        PostgresStorage::list_record_types_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError> {
        PostgresStorage::list_rrsets_in_conn(&mut self.conn.borrow_mut(), page)
    }
    fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError> {
        PostgresStorage::list_records_in_conn(&mut self.conn.borrow_mut(), page, filter)
    }
    fn get_record(&self, record_id: Uuid) -> Result<RecordInstance, AppError> {
        PostgresStorage::get_record_in_conn(&mut self.conn.borrow_mut(), record_id)
    }
    fn get_rrset(&self, rrset_id: Uuid) -> Result<RecordRrset, AppError> {
        PostgresStorage::get_rrset_in_conn(&mut self.conn.borrow_mut(), rrset_id)
    }
    fn list_records_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<RecordInstance>, AppError> {
        PostgresStorage::list_records_for_hosts_in_conn(&mut self.conn.borrow_mut(), hosts)
    }
    fn create_record_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError> {
        PostgresStorage::create_record_type_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError> {
        PostgresStorage::create_record_in_conn(&mut self.conn.borrow_mut(), command)
    }
    fn update_record(
        &self,
        record_id: Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError> {
        PostgresStorage::update_record_in_conn(&mut self.conn.borrow_mut(), record_id, command)
    }
    fn delete_record(&self, record_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_record_in_conn(&mut self.conn.borrow_mut(), record_id)
    }
    fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError> {
        PostgresStorage::delete_record_type_in_conn(&mut self.conn.borrow_mut(), name)
    }
    fn delete_rrset(&self, rrset_id: Uuid) -> Result<(), AppError> {
        PostgresStorage::delete_rrset_in_conn(&mut self.conn.borrow_mut(), rrset_id)
    }
    fn find_records_by_owner(
        &self,
        owner_id: Uuid,
    ) -> Result<Vec<RecordInstance>, AppError> {
        PostgresStorage::find_records_by_owner_in_conn(&mut self.conn.borrow_mut(), owner_id)
    }
    fn delete_records_by_owner(&self, owner_id: Uuid) -> Result<u64, AppError> {
        PostgresStorage::delete_records_by_owner_in_conn(&mut self.conn.borrow_mut(), owner_id)
    }
    fn delete_records_by_owner_name_and_type(
        &self,
        owner_name: &DnsName,
        type_name: &RecordTypeName,
    ) -> Result<u64, AppError> {
        PostgresStorage::delete_records_by_owner_name_and_type_in_conn(
            &mut self.conn.borrow_mut(),
            owner_name,
            type_name,
        )
    }
    fn rename_record_owner(
        &self,
        owner_id: Uuid,
        new_name: &DnsName,
    ) -> Result<u64, AppError> {
        PostgresStorage::rename_record_owner_in_conn(
            &mut self.conn.borrow_mut(),
            owner_id,
            new_name,
        )
    }
}

#[async_trait]
impl TransactionRunner for PostgresStorage {
    async fn run_transaction(
        &self,
        work: Box<dyn ErasedTxWork>,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<Box<dyn std::any::Any + Send>, AppError, _>(|conn| {
                    let tx = PgTxStorage::new(conn);
                    work.run(&tx)
                })
            })
            .await
    }
}
