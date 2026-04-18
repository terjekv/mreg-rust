pub mod attachments;
pub mod bacnet;
pub mod communities;
pub mod exports;
pub mod host_community_assignments;
pub mod host_contacts;
pub mod host_groups;
pub mod host_policy;
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

use serde_json::Value;
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
    events::{DomainEvent, EventSinkClient},
    storage::{
        AttachmentCommunityAssignmentStore, AttachmentStore, AuditStore, BacnetStore,
        CommunityStore, DynStorage, ExportStore, HostCommunityAssignmentStore, HostContactStore,
        HostGroupStore, HostPolicyStore, HostStore, ImportStore, LabelStore, NameServerStore,
        NetworkPolicyStore, NetworkStore, PtrOverrideStore, RecordStore, TaskStore, ZoneStore,
    },
};

/// Record an audit event and, on success, emit a domain event mirroring the
/// persisted row. Audit is the source of truth (see `docs/event-system.md`):
/// if persistence fails, no event is emitted, so external sinks never see a
/// mutation that has no audit record to reconcile against.
pub async fn record_and_emit(
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    event: CreateHistoryEvent,
) {
    let resource_kind = event.resource_kind().to_string();
    let resource_name = event.resource_name().to_string();
    let action = event.action().to_string();

    match audit.record_event(event).await {
        Ok(history) => events.emit(&DomainEvent::from(&history)).await,
        Err(error) => tracing::warn!(
            %resource_kind,
            %resource_name,
            %action,
            %error,
            "failed to record audit event; skipping event emission"
        ),
    }
}

/// Build a `CreateHistoryEvent` with the canonical `actor::SYSTEM` actor and
/// hand it to `record_and_emit`. Replaces the repeated `CreateHistoryEvent::new(
/// "system", ...)` + `record_and_emit(...)` pair across service mutations.
pub async fn audit_mutation(
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    resource_kind: &str,
    action: &str,
    resource_id: Option<Uuid>,
    resource_name: impl Into<String>,
    payload: Value,
) {
    let event = CreateHistoryEvent::new(
        crate::audit::actor::SYSTEM,
        resource_kind,
        resource_id,
        resource_name,
        action,
        payload,
    );
    record_and_emit(audit, events, event).await;
}

#[cfg(test)]
mod record_and_emit_tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use uuid::Uuid;

    use crate::{
        audit::{CreateHistoryEvent, HistoryEvent},
        domain::pagination::{Page, PageRequest},
        errors::AppError,
        events::{DomainEvent, EventSink, EventSinkClient},
        storage::AuditStore,
    };

    struct StaticAuditStore {
        id: Uuid,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    #[async_trait]
    impl AuditStore for StaticAuditStore {
        async fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
            Ok(HistoryEvent::restore(
                self.id,
                event.actor().to_string(),
                event.resource_kind().to_string(),
                event.resource_id(),
                event.resource_name().to_string(),
                event.action().to_string(),
                event.data().clone(),
                self.created_at,
            ))
        }

        async fn list_events(&self, _page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
            unreachable!("list_events not exercised by these tests")
        }
    }

    struct FailingAuditStore;

    #[async_trait]
    impl AuditStore for FailingAuditStore {
        async fn record_event(&self, _event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
            Err(AppError::internal("simulated audit failure"))
        }

        async fn list_events(&self, _page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
            unreachable!("list_events not exercised by these tests")
        }
    }

    struct CollectorSink {
        events: Arc<Mutex<Vec<DomainEvent>>>,
    }

    #[async_trait]
    impl EventSink for CollectorSink {
        async fn emit(&self, event: &DomainEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn collector() -> (EventSinkClient, Arc<Mutex<Vec<DomainEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let client = EventSinkClient::with_sink(Arc::new(CollectorSink {
            events: events.clone(),
        }));
        (client, events)
    }

    fn sample_event() -> CreateHistoryEvent {
        CreateHistoryEvent::new(
            "system",
            "label",
            None,
            "prod",
            "create",
            json!({"name": "prod"}),
        )
    }

    async fn wait_for_events(events: &Arc<Mutex<Vec<DomainEvent>>>) -> Vec<DomainEvent> {
        for _ in 0..100 {
            let snapshot = events.lock().unwrap().clone();
            if !snapshot.is_empty() {
                return snapshot;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("timed out waiting for emitted event");
    }

    #[tokio::test]
    async fn emitted_event_mirrors_persisted_row_on_success() {
        let id = Uuid::new_v4();
        let created_at = Utc.with_ymd_and_hms(2026, 4, 18, 12, 0, 0).unwrap();
        let audit = StaticAuditStore { id, created_at };
        let (events, captured) = collector();

        super::record_and_emit(&audit, &events, sample_event()).await;

        let emitted = wait_for_events(&captured).await;
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].id, id);
        assert_eq!(emitted[0].timestamp, created_at);
    }

    #[tokio::test]
    async fn no_event_emitted_when_audit_persistence_fails() {
        let audit = FailingAuditStore;
        let (events, captured) = collector();

        super::record_and_emit(&audit, &events, sample_event()).await;

        // Give the spawned emit task time to run if it had been scheduled.
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        assert!(
            captured.lock().unwrap().is_empty(),
            "no event should be emitted when audit persistence fails"
        );
    }
}

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
            store: self.storage.labels(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn nameservers(&self) -> NameServerService<'_> {
        NameServerService {
            store: self.storage.nameservers(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn zones(&self) -> ZoneService<'_> {
        ZoneService {
            store: self.storage.zones(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn networks(&self) -> NetworkService<'_> {
        NetworkService {
            store: self.storage.networks(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn hosts(&self) -> HostService<'_> {
        HostService {
            store: self.storage.hosts(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn attachments(&self) -> AttachmentService<'_> {
        AttachmentService {
            store: self.storage.attachments(),
            aca_store: self.storage.attachment_community_assignments(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn records(&self) -> RecordService<'_> {
        RecordService {
            store: self.storage.records(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn host_contacts(&self) -> HostContactService<'_> {
        HostContactService {
            store: self.storage.host_contacts(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn host_groups(&self) -> HostGroupService<'_> {
        HostGroupService {
            store: self.storage.host_groups(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn bacnet(&self) -> BacnetService<'_> {
        BacnetService {
            store: self.storage.bacnet(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn ptr_overrides(&self) -> PtrOverrideService<'_> {
        PtrOverrideService {
            store: self.storage.ptr_overrides(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn network_policies(&self) -> NetworkPolicyService<'_> {
        NetworkPolicyService {
            store: self.storage.network_policies(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn communities(&self) -> CommunityService<'_> {
        CommunityService {
            store: self.storage.communities(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn host_community_assignments(&self) -> HostCommunityAssignmentService<'_> {
        HostCommunityAssignmentService {
            store: self.storage.host_community_assignments(),
            audit: self.storage.audit(),
            events: &self.events,
        }
    }

    pub fn host_policy(&self) -> HostPolicyService<'_> {
        HostPolicyService {
            store: self.storage.host_policy(),
            audit: self.storage.audit(),
            events: &self.events,
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
    store: &'a (dyn LabelStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl LabelService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        labels::list(self.store, page).await
    }
    pub async fn create(&self, command: CreateLabel) -> Result<Label, AppError> {
        labels::create(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, name: &LabelName) -> Result<Label, AppError> {
        labels::get(self.store, name).await
    }
    pub async fn update(&self, name: &LabelName, command: UpdateLabel) -> Result<Label, AppError> {
        labels::update(self.store, self.audit, self.events, name, command).await
    }
    pub async fn delete(&self, name: &LabelName) -> Result<(), AppError> {
        labels::delete(self.store, self.audit, self.events, name).await
    }
}

pub struct NameServerService<'a> {
    store: &'a (dyn NameServerStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl NameServerService<'_> {
    pub async fn list(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError> {
        nameservers::list(self.store, page).await
    }
    pub async fn create(&self, command: CreateNameServer) -> Result<NameServer, AppError> {
        nameservers::create(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, name: &DnsName) -> Result<NameServer, AppError> {
        nameservers::get(self.store, name).await
    }
    pub async fn update(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        nameservers::update(self.store, self.audit, self.events, name, command).await
    }
    pub async fn delete(&self, name: &DnsName) -> Result<(), AppError> {
        nameservers::delete(self.store, self.audit, self.events, name).await
    }
}

pub struct ZoneService<'a> {
    store: &'a (dyn ZoneStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl ZoneService<'_> {
    pub async fn list_forward(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError> {
        zones::list_forward(self.store, page).await
    }
    pub async fn create_forward(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        zones::create_forward(self.store, self.audit, self.events, command).await
    }
    pub async fn get_forward(&self, name: &ZoneName) -> Result<ForwardZone, AppError> {
        zones::get_forward(self.store, name).await
    }
    pub async fn update_forward(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        zones::update_forward(self.store, self.audit, self.events, name, command).await
    }
    pub async fn delete_forward(&self, name: &ZoneName) -> Result<(), AppError> {
        zones::delete_forward(self.store, self.audit, self.events, name).await
    }
    pub async fn list_reverse(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError> {
        zones::list_reverse(self.store, page).await
    }
    pub async fn create_reverse(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        zones::create_reverse(self.store, self.audit, self.events, command).await
    }
    pub async fn get_reverse(&self, name: &ZoneName) -> Result<ReverseZone, AppError> {
        zones::get_reverse(self.store, name).await
    }
    pub async fn update_reverse(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        zones::update_reverse(self.store, self.audit, self.events, name, command).await
    }
    pub async fn delete_reverse(&self, name: &ZoneName) -> Result<(), AppError> {
        zones::delete_reverse(self.store, self.audit, self.events, name).await
    }
    pub async fn list_forward_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        zones::list_forward_delegations(self.store, zone_name, page).await
    }
    pub async fn create_forward_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        zones::create_forward_delegation(self.store, self.audit, self.events, command).await
    }
    pub async fn delete_forward_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        zones::delete_forward_delegation(self.store, self.audit, self.events, delegation_id).await
    }
    pub async fn list_reverse_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        zones::list_reverse_delegations(self.store, zone_name, page).await
    }
    pub async fn create_reverse_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        zones::create_reverse_delegation(self.store, self.audit, self.events, command).await
    }
    pub async fn delete_reverse_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        zones::delete_reverse_delegation(self.store, self.audit, self.events, delegation_id).await
    }
}

pub struct NetworkService<'a> {
    store: &'a (dyn NetworkStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl NetworkService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        networks::list(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreateNetwork) -> Result<Network, AppError> {
        networks::create(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, cidr: &CidrValue) -> Result<Network, AppError> {
        networks::get(self.store, cidr).await
    }
    pub async fn update(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        networks::update(self.store, self.audit, self.events, cidr, command).await
    }
    pub async fn delete(&self, cidr: &CidrValue) -> Result<(), AppError> {
        networks::delete(self.store, self.audit, self.events, cidr).await
    }
    pub async fn list_excluded_ranges(
        &self,
        cidr: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        networks::list_excluded_ranges(self.store, cidr, page).await
    }
    pub async fn add_excluded_range(
        &self,
        cidr: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        networks::add_excluded_range(self.store, self.audit, self.events, cidr, command).await
    }
    pub async fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        networks::list_used_addresses(self.store, cidr).await
    }
    pub async fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        networks::list_unused_addresses(self.store, cidr, limit).await
    }
    pub async fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError> {
        self.store.count_unused_addresses(cidr).await
    }
}

pub struct HostService<'a> {
    store: &'a (dyn HostStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl HostService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError> {
        hosts::list(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreateHost) -> Result<Host, AppError> {
        hosts::create(self.store, command, self.audit, self.events).await
    }
    pub async fn get(&self, name: &Hostname) -> Result<Host, AppError> {
        hosts::get(self.store, name).await
    }
    pub async fn get_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError> {
        hosts::get_auth_context(self.store, name).await
    }
    pub async fn update(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError> {
        hosts::update(self.store, name, command, self.audit, self.events).await
    }
    pub async fn delete(&self, name: &Hostname) -> Result<(), AppError> {
        hosts::delete(self.store, name, self.audit, self.events).await
    }
    pub async fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        hosts::list_ip_addresses(self.store, page).await
    }
    pub async fn list_host_ip_addresses(
        &self,
        name: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        hosts::list_host_ip_addresses(self.store, name, page).await
    }
    pub async fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        hosts::assign_ip_address(self.store, command, self.audit, self.events).await
    }
    pub async fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        hosts::update_ip_address(self.store, address, command, self.audit, self.events).await
    }
    pub async fn unassign_ip_address(&self, address: &IpAddressValue) -> Result<(), AppError> {
        hosts::unassign_ip_address(self.store, address, self.audit, self.events).await
    }
}

pub struct AttachmentService<'a> {
    store: &'a (dyn AttachmentStore + Send + Sync),
    aca_store: &'a (dyn AttachmentCommunityAssignmentStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl AttachmentService<'_> {
    pub async fn list_attachments_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        self.store.list_attachments_for_host(host_name).await
    }
    pub async fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        self.store.list_attachments_for_network(network).await
    }
    pub async fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError> {
        self.store.get_attachment(attachment_id).await
    }
    pub async fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        attachments::create_attachment(self.store, command, self.audit, self.events).await
    }
    pub async fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        attachments::update_attachment(self.store, attachment_id, command, self.audit, self.events)
            .await
    }
    pub async fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError> {
        attachments::delete_attachment(self.store, attachment_id, self.audit, self.events).await
    }
    pub async fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        attachments::create_attachment_dhcp_identifier(self.store, command, self.audit, self.events)
            .await
    }
    pub async fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        self.store
            .list_attachment_dhcp_identifiers(attachment_id)
            .await
    }
    pub async fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        self.store
            .list_attachment_dhcp_identifiers_for_attachments(attachment_ids)
            .await
    }
    pub async fn delete_attachment_dhcp_identifier(
        &self,
        attachment_id: Uuid,
        identifier_id: Uuid,
    ) -> Result<(), AppError> {
        attachments::delete_attachment_dhcp_identifier(
            self.store,
            attachment_id,
            identifier_id,
            self.audit,
            self.events,
        )
        .await
    }
    pub async fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        attachments::create_attachment_prefix_reservation(
            self.store,
            command,
            self.audit,
            self.events,
        )
        .await
    }
    pub async fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        self.store
            .list_attachment_prefix_reservations(attachment_id)
            .await
    }
    pub async fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        self.store
            .list_attachment_prefix_reservations_for_attachments(attachment_ids)
            .await
    }
    pub async fn delete_attachment_prefix_reservation(
        &self,
        attachment_id: Uuid,
        reservation_id: Uuid,
    ) -> Result<(), AppError> {
        attachments::delete_attachment_prefix_reservation(
            self.store,
            attachment_id,
            reservation_id,
            self.audit,
            self.events,
        )
        .await
    }
    pub async fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &crate::domain::filters::AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        self.aca_store
            .list_attachment_community_assignments(page, filter)
            .await
    }
    pub async fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        self.aca_store
            .list_attachment_community_assignments_for_attachments(attachment_ids)
            .await
    }
    pub async fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        attachments::create_attachment_community_assignment(
            self.aca_store,
            command,
            self.audit,
            self.events,
        )
        .await
    }
    pub async fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        self.aca_store
            .get_attachment_community_assignment(assignment_id)
            .await
    }
    pub async fn delete_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<(), AppError> {
        attachments::delete_attachment_community_assignment(
            self.aca_store,
            assignment_id,
            self.audit,
            self.events,
        )
        .await
    }
}

pub struct RecordService<'a> {
    store: &'a (dyn RecordStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl RecordService<'_> {
    pub async fn list_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError> {
        records::list_types(self.store, page).await
    }
    pub async fn create_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError> {
        records::create_type(self.store, self.audit, self.events, command).await
    }
    pub async fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError> {
        records::delete_record_type(self.store, self.audit, self.events, name).await
    }
    pub async fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError> {
        records::list_records(self.store, page, filter).await
    }
    pub async fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError> {
        records::list_rrsets(self.store, page).await
    }
    pub async fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError> {
        records::create_record(self.store, self.audit, self.events, command).await
    }
    pub async fn get_record(&self, record_id: Uuid) -> Result<RecordInstance, AppError> {
        records::get_record(self.store, record_id).await
    }
    pub async fn get_rrset(&self, rrset_id: Uuid) -> Result<RecordRrset, AppError> {
        records::get_rrset(self.store, rrset_id).await
    }
    pub async fn update_record(
        &self,
        record_id: Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError> {
        records::update_record(self.store, self.audit, self.events, record_id, command).await
    }
    pub async fn delete_record(&self, record_id: Uuid) -> Result<(), AppError> {
        records::delete_record(self.store, self.audit, self.events, record_id).await
    }
    pub async fn delete_rrset(&self, rrset_id: Uuid) -> Result<(), AppError> {
        records::delete_rrset(self.store, self.audit, self.events, rrset_id).await
    }
}

pub struct HostContactService<'a> {
    store: &'a (dyn HostContactStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl HostContactService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError> {
        host_contacts::list_host_contacts(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreateHostContact) -> Result<HostContact, AppError> {
        host_contacts::create_host_contact(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, email: &EmailAddressValue) -> Result<HostContact, AppError> {
        host_contacts::get_host_contact(self.store, email).await
    }
    pub async fn delete(&self, email: &EmailAddressValue) -> Result<(), AppError> {
        host_contacts::delete_host_contact(self.store, self.audit, self.events, email).await
    }
}

pub struct HostGroupService<'a> {
    store: &'a (dyn HostGroupStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl HostGroupService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError> {
        host_groups::list_host_groups(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreateHostGroup) -> Result<HostGroup, AppError> {
        host_groups::create_host_group(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, name: &HostGroupName) -> Result<HostGroup, AppError> {
        host_groups::get_host_group(self.store, name).await
    }
    pub async fn delete(&self, name: &HostGroupName) -> Result<(), AppError> {
        host_groups::delete_host_group(self.store, self.audit, self.events, name).await
    }
}

pub struct BacnetService<'a> {
    store: &'a (dyn BacnetStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl BacnetService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        bacnet::list_bacnet_ids(self.store, page, filter).await
    }
    pub async fn create(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError> {
        bacnet::create_bacnet_id(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, bacnet_id: BacnetIdentifier) -> Result<BacnetIdAssignment, AppError> {
        bacnet::get_bacnet_id(self.store, bacnet_id).await
    }
    pub async fn delete(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        bacnet::delete_bacnet_id(self.store, self.audit, self.events, bacnet_id).await
    }
}

pub struct PtrOverrideService<'a> {
    store: &'a (dyn PtrOverrideStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl PtrOverrideService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError> {
        ptr_overrides::list_ptr_overrides(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreatePtrOverride) -> Result<PtrOverride, AppError> {
        ptr_overrides::create_ptr_override(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, address: &IpAddressValue) -> Result<PtrOverride, AppError> {
        ptr_overrides::get_ptr_override(self.store, address).await
    }
    pub async fn delete(&self, address: &IpAddressValue) -> Result<(), AppError> {
        ptr_overrides::delete_ptr_override(self.store, self.audit, self.events, address).await
    }
}

pub struct NetworkPolicyService<'a> {
    store: &'a (dyn NetworkPolicyStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl NetworkPolicyService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        network_policies::list_network_policies(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreateNetworkPolicy) -> Result<NetworkPolicy, AppError> {
        network_policies::create_network_policy(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, name: &NetworkPolicyName) -> Result<NetworkPolicy, AppError> {
        network_policies::get_network_policy(self.store, name).await
    }
    pub async fn delete(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        network_policies::delete_network_policy(self.store, self.audit, self.events, name).await
    }
}

pub struct CommunityService<'a> {
    store: &'a (dyn CommunityStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl CommunityService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError> {
        communities::list_communities(self.store, page, filter).await
    }
    pub async fn create(&self, command: CreateCommunity) -> Result<Community, AppError> {
        communities::create_community(self.store, self.audit, self.events, command).await
    }
    pub async fn get(&self, community_id: Uuid) -> Result<Community, AppError> {
        communities::get_community(self.store, community_id).await
    }
    pub async fn delete(&self, community_id: Uuid) -> Result<(), AppError> {
        communities::delete_community(self.store, self.audit, self.events, community_id).await
    }
    pub async fn find_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError> {
        communities::find_community_by_names(self.store, policy_name, community_name).await
    }
}

pub struct HostCommunityAssignmentService<'a> {
    store: &'a (dyn HostCommunityAssignmentStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl HostCommunityAssignmentService<'_> {
    pub async fn list(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError> {
        host_community_assignments::list_host_community_assignments(self.store, page, filter).await
    }
    pub async fn create(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError> {
        host_community_assignments::create_host_community_assignment(
            self.store,
            self.audit,
            self.events,
            command,
        )
        .await
    }
    pub async fn get(&self, mapping_id: Uuid) -> Result<HostCommunityAssignment, AppError> {
        host_community_assignments::get_host_community_assignment(self.store, mapping_id).await
    }
    pub async fn delete(&self, mapping_id: Uuid) -> Result<(), AppError> {
        host_community_assignments::delete_host_community_assignment(
            self.store,
            self.audit,
            self.events,
            mapping_id,
        )
        .await
    }
}

pub struct HostPolicyService<'a> {
    store: &'a (dyn HostPolicyStore + Send + Sync),
    audit: &'a (dyn AuditStore + Send + Sync),
    events: &'a EventSinkClient,
}

impl HostPolicyService<'_> {
    pub async fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError> {
        host_policy::list_atoms(self.store, page).await
    }
    pub async fn create_atom(
        &self,
        command: CreateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        host_policy::create_atom(self.store, self.audit, self.events, command).await
    }
    pub async fn get_atom(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError> {
        host_policy::get_atom(self.store, name).await
    }
    pub async fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        host_policy::update_atom(self.store, self.audit, self.events, name, command).await
    }
    pub async fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError> {
        host_policy::delete_atom(self.store, self.audit, self.events, name).await
    }
    pub async fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError> {
        host_policy::list_roles(self.store, page).await
    }
    pub async fn list_roles_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        self.store.list_roles_for_host(host_name).await
    }
    pub async fn create_role(
        &self,
        command: CreateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        host_policy::create_role(self.store, self.audit, self.events, command).await
    }
    pub async fn get_role(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError> {
        host_policy::get_role(self.store, name).await
    }
    pub async fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        host_policy::update_role(self.store, self.audit, self.events, name, command).await
    }
    pub async fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError> {
        host_policy::delete_role(self.store, self.audit, self.events, name).await
    }
    pub async fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        host_policy::add_atom_to_role(self.store, self.audit, self.events, role_name, atom_name)
            .await
    }
    pub async fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        host_policy::remove_atom_from_role(
            self.store,
            self.audit,
            self.events,
            role_name,
            atom_name,
        )
        .await
    }
    pub async fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        host_policy::add_host_to_role(self.store, self.audit, self.events, role_name, host_name)
            .await
    }
    pub async fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        host_policy::remove_host_from_role(
            self.store,
            self.audit,
            self.events,
            role_name,
            host_name,
        )
        .await
    }
    pub async fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        host_policy::add_label_to_role(self.store, self.audit, self.events, role_name, label_name)
            .await
    }
    pub async fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        host_policy::remove_label_from_role(
            self.store,
            self.audit,
            self.events,
            role_name,
            label_name,
        )
        .await
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
