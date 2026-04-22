mod attachments;
mod audit;
mod auth_sessions;
mod bacnet;
mod communities;
mod exports;
mod host_community_assignments;
mod host_contacts;
mod host_groups;
mod host_policy;
mod host_views;
mod hosts;
mod imports;
mod labels;
mod nameservers;
mod network_policies;
mod networks;
mod ptr_overrides;
mod records;
mod tasks;
mod zones;

use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    audit::HistoryEvent,
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            HostAttachment,
        },
        bacnet::BacnetIdAssignment,
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
        pagination::{Page, PageRequest, SortDirection},
        ptr_override::PtrOverride,
        resource_records::{
            RecordInstance, RecordRrset, RecordTypeDefinition, built_in_record_types,
        },
        tasks::TaskEnvelope,
        zone::{ForwardZone, ForwardZoneDelegation, ReverseZone, ReverseZoneDelegation},
    },
    errors::AppError,
    storage::{
        AttachmentCommunityAssignmentStore, AttachmentStore, AuditStore, AuthSessionStore,
        BacnetStore, CommunityStore, ExportStore, HostCommunityAssignmentStore, HostContactStore,
        HostGroupStore, HostPolicyStore, HostStore, HostViewStore, ImportStore, LabelStore,
        NameServerStore, NetworkPolicyStore, NetworkStore, PtrOverrideStore, RecordStore, Storage,
        StorageBackendKind, StorageCapabilities, StorageHealthReport, TaskStore, ZoneStore,
        has_id::HasId,
    },
};

pub(super) fn paginate_by_cursor<T: HasId>(
    items: Vec<T>,
    page: &PageRequest,
) -> Result<Page<T>, AppError> {
    let total = items.len() as u64;
    let start = if let Some(cursor) = page.after() {
        items
            .iter()
            .position(|item| item.id() == cursor)
            .map(|pos| pos + 1)
            .unwrap_or(0)
    } else {
        0
    };
    let limit = page.limit() as usize;
    let take_count = limit.saturating_add(1);
    let page_items: Vec<T> = items.into_iter().skip(start).take(take_count).collect();
    let has_more = page_items.len() > limit;
    let mut page_items = page_items;
    if has_more {
        page_items.pop();
    }
    let next_cursor = if has_more {
        page_items.last().map(|item| item.id())
    } else {
        None
    };
    Ok(Page {
        items: page_items,
        total,
        next_cursor,
    })
}

/// Simple pagination for types without a UUID id (e.g. BacnetIdAssignment).
pub(super) fn paginate_simple<T>(items: Vec<T>, page: &PageRequest) -> Page<T> {
    let total = items.len() as u64;
    let limit = page.limit() as usize;
    // For types without UUID cursor, we only support offset-less first-page pagination
    // or returning all items. The cursor is ignored.
    let page_items: Vec<T> = items.into_iter().take(limit).collect();
    Page {
        items: page_items,
        total,
        next_cursor: None,
    }
}

pub(super) fn sort_items<T: HasId>(
    items: &mut [T],
    page: &PageRequest,
    valid_fields: &[&str],
    key_fn: impl Fn(&T, &str) -> String,
) -> Result<(), crate::errors::AppError> {
    if let Some(field) = page.sort_by()
        && !valid_fields.contains(&field)
    {
        return Err(crate::errors::AppError::validation(format!(
            "unsupported sort_by field: {field}"
        )));
    }
    let field = page.sort_by().unwrap_or("name");
    let descending = *page.sort_direction() == SortDirection::Desc;
    items.sort_by(|a, b| {
        let cmp = key_fn(a, field).cmp(&key_fn(b, field));
        if descending { cmp.reverse() } else { cmp }
    });
    Ok(())
}

pub(super) fn sort_and_paginate<T: HasId>(
    mut items: Vec<T>,
    page: &PageRequest,
    valid_fields: &[&str],
    key_fn: impl Fn(&T, &str) -> String,
) -> Result<Page<T>, AppError> {
    sort_items(&mut items, page, valid_fields, key_fn)?;
    paginate_by_cursor(items, page)
}

#[derive(Clone)]
pub(super) struct StoredImportBatch {
    pub(super) batch: crate::domain::imports::ImportBatch,
    pub(super) summary: ImportBatchSummary,
}

#[derive(Clone, Default)]
pub(super) struct MemoryState {
    pub(super) host_policy_atoms: BTreeMap<String, HostPolicyAtom>,
    pub(super) host_policy_roles: BTreeMap<String, HostPolicyRole>,
    pub(super) labels: BTreeMap<String, Label>,
    pub(super) nameservers: BTreeMap<String, NameServer>,
    pub(super) forward_zones: BTreeMap<String, ForwardZone>,
    pub(super) reverse_zones: BTreeMap<String, ReverseZone>,
    pub(super) forward_zone_delegations: BTreeMap<Uuid, ForwardZoneDelegation>,
    pub(super) reverse_zone_delegations: BTreeMap<Uuid, ReverseZoneDelegation>,
    pub(super) networks: BTreeMap<String, Network>,
    pub(super) excluded_ranges: BTreeMap<String, Vec<ExcludedRange>>,
    pub(super) hosts: BTreeMap<String, Host>,
    pub(super) host_attachments: BTreeMap<Uuid, HostAttachment>,
    pub(super) ip_addresses: BTreeMap<String, IpAddressAssignment>,
    pub(super) host_contacts: BTreeMap<String, HostContact>,
    pub(super) host_groups: BTreeMap<String, HostGroup>,
    pub(super) bacnet_ids: BTreeMap<u32, BacnetIdAssignment>,
    pub(super) ptr_overrides: BTreeMap<String, PtrOverride>,
    pub(super) network_policies: BTreeMap<String, NetworkPolicy>,
    pub(super) communities: BTreeMap<Uuid, Community>,
    pub(super) attachment_community_assignments: BTreeMap<Uuid, AttachmentCommunityAssignment>,
    pub(super) host_community_assignments: BTreeMap<Uuid, HostCommunityAssignment>,
    pub(super) attachment_dhcp_identifiers: BTreeMap<Uuid, AttachmentDhcpIdentifier>,
    pub(super) attachment_prefix_reservations: BTreeMap<Uuid, AttachmentPrefixReservation>,
    pub(super) tasks: BTreeMap<Uuid, TaskEnvelope>,
    pub(super) imports: BTreeMap<Uuid, StoredImportBatch>,
    pub(super) export_templates: BTreeMap<String, ExportTemplate>,
    pub(super) export_runs: BTreeMap<Uuid, ExportRun>,
    pub(super) record_types: BTreeMap<String, RecordTypeDefinition>,
    pub(super) rrsets: BTreeMap<Uuid, RecordRrset>,
    pub(super) records: Vec<RecordInstance>,
    pub(super) history_events: Vec<HistoryEvent>,
    pub(super) revoked_tokens: BTreeMap<String, (String, chrono::DateTime<Utc>)>,
    pub(super) principal_revoked_before: BTreeMap<String, chrono::DateTime<Utc>>,
}

#[derive(Clone, Default)]
pub(crate) struct MemoryStorage {
    pub(super) state: Arc<RwLock<MemoryState>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        let mut state = MemoryState::default();
        let now = Utc::now();
        if let Ok(builtins) = built_in_record_types() {
            for command in builtins {
                let definition = RecordTypeDefinition::restore(
                    Uuid::new_v4(),
                    command.name().clone(),
                    command.dns_type(),
                    command.schema().clone(),
                    command.built_in(),
                    now,
                    now,
                );
                state
                    .record_types
                    .insert(definition.name().as_str().to_string(), definition);
            }
        }

        // Seed built-in export templates
        if let Ok(builtins) = crate::domain::builtin_export_templates::built_in_export_templates() {
            for (command, built_in) in builtins {
                let key = command.name().to_ascii_lowercase();
                if let std::collections::btree_map::Entry::Vacant(e) =
                    state.export_templates.entry(key)
                    && let Ok(template) = ExportTemplate::restore(
                        Uuid::new_v4(),
                        command.name(),
                        command.description(),
                        command.engine(),
                        command.scope(),
                        command.body(),
                        command.metadata().clone(),
                        built_in,
                    )
                {
                    e.insert(template);
                }
            }
        }

        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }
}

/// Bump zone serial within the same write lock. Tries forward zones first, then reverse.
pub(super) fn bump_zone_serial_in_state(state: &mut MemoryState, zone_id: Uuid) {
    let now = Utc::now();
    if let Some(zone) = state.forward_zones.values_mut().find(|z| z.id() == zone_id) {
        if let Ok(next) = zone.serial_no().next_rfc1912(now.date_naive())
            && let Ok(updated) = ForwardZone::restore(
                zone.id(),
                zone.name().clone(),
                true,
                zone.primary_ns().clone(),
                zone.nameservers().to_vec(),
                zone.email().clone(),
                next,
                now,
                zone.refresh(),
                zone.retry(),
                zone.expire(),
                zone.soa_ttl(),
                zone.default_ttl(),
                zone.created_at(),
                now,
            )
        {
            *zone = updated;
        }
    } else if let Some(zone) = state.reverse_zones.values_mut().find(|z| z.id() == zone_id)
        && let Ok(next) = zone.serial_no().next_rfc1912(now.date_naive())
        && let Ok(updated) = ReverseZone::restore(
            zone.id(),
            zone.name().clone(),
            zone.network().cloned(),
            true,
            zone.primary_ns().clone(),
            zone.nameservers().to_vec(),
            zone.email().clone(),
            next,
            now,
            zone.refresh(),
            zone.retry(),
            zone.expire(),
            zone.soa_ttl(),
            zone.default_ttl(),
            zone.created_at(),
            now,
        )
    {
        *zone = updated;
    }
}

/// Delete all records owned by a given owner_id within the same lock.
/// Also cleans up empty rrsets.
pub(super) fn delete_records_by_owner_in_state(state: &mut MemoryState, owner_id: Uuid) -> u64 {
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for record in state.records.drain(..) {
        if record.owner_id() == Some(owner_id) {
            removed.push(record);
        } else {
            kept.push(record);
        }
    }
    state.records = kept;
    let count = removed.len() as u64;
    let rrset_ids: HashSet<Uuid> = removed.iter().map(|r| r.rrset_id()).collect();
    for rrset_id in rrset_ids {
        if !state.records.iter().any(|r| r.rrset_id() == rrset_id) {
            state.rrsets.remove(&rrset_id);
        }
    }
    count
}

/// Delete records matching owner_name (case-insensitive) and type_name.
/// Also cleans up empty rrsets.
pub(super) fn delete_records_by_name_and_type_in_state(
    state: &mut MemoryState,
    owner_name: &str,
    type_name: &str,
) -> u64 {
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for record in state.records.drain(..) {
        if record.owner_name().eq_ignore_ascii_case(owner_name)
            && record.type_name().as_str().eq_ignore_ascii_case(type_name)
        {
            removed.push(record);
        } else {
            kept.push(record);
        }
    }
    state.records = kept;
    let count = removed.len() as u64;
    let rrset_ids: HashSet<Uuid> = removed.iter().map(|r| r.rrset_id()).collect();
    for rrset_id in rrset_ids {
        if !state.records.iter().any(|r| r.rrset_id() == rrset_id) {
            state.rrsets.remove(&rrset_id);
        }
    }
    count
}

#[async_trait]
impl Storage for MemoryStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Memory
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            persistent: false,
            strong_transactions: false,
            native_network_types: false,
            skip_locked_task_claiming: false,
            intended_for: vec![
                "unit_tests".to_string(),
                "lightweight_dev".to_string(),
                "service_scaffolding".to_string(),
            ],
        }
    }

    async fn health(&self) -> Result<StorageHealthReport, AppError> {
        Ok(StorageHealthReport {
            backend: StorageBackendKind::Memory,
            configured: true,
            ready: true,
            detail: "in-memory storage is active".to_string(),
        })
    }

    fn labels(&self) -> &(dyn LabelStore + Send + Sync) {
        self
    }

    fn nameservers(&self) -> &(dyn NameServerStore + Send + Sync) {
        self
    }

    fn zones(&self) -> &(dyn ZoneStore + Send + Sync) {
        self
    }

    fn networks(&self) -> &(dyn NetworkStore + Send + Sync) {
        self
    }

    fn hosts(&self) -> &(dyn HostStore + Send + Sync) {
        self
    }

    fn attachments(&self) -> &(dyn AttachmentStore + Send + Sync) {
        self
    }

    fn host_contacts(&self) -> &(dyn HostContactStore + Send + Sync) {
        self
    }

    fn host_groups(&self) -> &(dyn HostGroupStore + Send + Sync) {
        self
    }

    fn bacnet(&self) -> &(dyn BacnetStore + Send + Sync) {
        self
    }

    fn ptr_overrides(&self) -> &(dyn PtrOverrideStore + Send + Sync) {
        self
    }

    fn network_policies(&self) -> &(dyn NetworkPolicyStore + Send + Sync) {
        self
    }

    fn communities(&self) -> &(dyn CommunityStore + Send + Sync) {
        self
    }

    fn attachment_community_assignments(
        &self,
    ) -> &(dyn AttachmentCommunityAssignmentStore + Send + Sync) {
        self
    }

    fn host_community_assignments(&self) -> &(dyn HostCommunityAssignmentStore + Send + Sync) {
        self
    }

    fn tasks(&self) -> &(dyn TaskStore + Send + Sync) {
        self
    }

    fn imports(&self) -> &(dyn ImportStore + Send + Sync) {
        self
    }

    fn exports(&self) -> &(dyn ExportStore + Send + Sync) {
        self
    }

    fn records(&self) -> &(dyn RecordStore + Send + Sync) {
        self
    }

    fn audit(&self) -> &(dyn AuditStore + Send + Sync) {
        self
    }

    fn auth_sessions(&self) -> &(dyn AuthSessionStore + Send + Sync) {
        self
    }

    fn host_policy(&self) -> &(dyn HostPolicyStore + Send + Sync) {
        self
    }

    fn host_views(&self) -> &(dyn HostViewStore + Send + Sync) {
        self
    }
}
