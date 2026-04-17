use std::collections::HashSet;

use async_trait::async_trait;
use chrono::Utc;
use minijinja::Environment;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::{
        filters::RecordFilter,
        pagination::{Page, PageRequest},
        resource_records::{
            CreateRecordInstance, CreateRecordTypeDefinition, DnsClass, ExistingRecordSummary,
            RecordInstance, RecordOwnerKind, RecordRrset, RecordTypeDefinition, UpdateRecord,
            ValidatedRecordContent, alias_target_names, validate_record_relationships,
        },
        types::{DnsName, RecordTypeName},
    },
    errors::AppError,
    storage::RecordStore,
};

use super::{
    MemoryState, MemoryStorage, bump_zone_serial_in_state, paginate_by_cursor, sort_items,
};

pub(super) fn create_record_in_state(
    state: &mut MemoryState,
    command: CreateRecordInstance,
) -> Result<RecordInstance, AppError> {
    let record_type = state
        .record_types
        .get(command.type_name().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "record type '{}' was not found",
                command.type_name().as_str()
            ))
        })?;

    let (anchor_id, anchor_name, zone_id) = resolve_record_owner(
        state,
        command.owner_kind(),
        command.anchor_name(),
        command.owner_name(),
    )?;
    let validated = record_type.validate_record_input(
        command.owner_name(),
        command.data(),
        command.raw_rdata(),
    )?;
    let same_owner_records = state
        .records
        .iter()
        .filter(|record| record.owner_name() == command.owner_name().as_str())
        .map(|record| {
            ExistingRecordSummary::new(
                record.type_name().clone(),
                record.ttl(),
                record.data().clone(),
                record.raw_rdata().cloned(),
            )
        })
        .collect::<Vec<_>>();
    let existing_rrset_id = state
        .rrsets
        .values()
        .find(|rrset| {
            rrset.type_id() == record_type.id()
                && rrset.owner_name() == command.owner_name()
                && rrset.dns_class() == &DnsClass::IN
        })
        .map(RecordRrset::id);
    let same_rrset_records = state
        .records
        .iter()
        .filter(|record| existing_rrset_id.is_some_and(|rrset_id| record.rrset_id() == rrset_id))
        .map(|record| {
            ExistingRecordSummary::new(
                record.type_name().clone(),
                record.ttl(),
                record.data().clone(),
                record.raw_rdata().cloned(),
            )
        })
        .collect::<Vec<_>>();
    let alias_owner_names = match &validated {
        ValidatedRecordContent::Structured(normalized) => {
            alias_target_names(normalized, record_type.name())
        }
        ValidatedRecordContent::RawRdata(_) => Vec::new(),
    }
    .into_iter()
    .filter(|target| {
        state.records.iter().any(|record| {
            record.type_name().as_str() == "CNAME" && record.owner_name() == target.as_str()
        })
    })
    .collect();
    validate_record_relationships(
        &record_type,
        command.ttl(),
        &validated,
        &same_owner_records,
        &same_rrset_records,
        &alias_owner_names,
    )?;
    let rendered = if let (Some(template), ValidatedRecordContent::Structured(normalized)) =
        (record_type.schema().render_template(), &validated)
    {
        let mut env = Environment::new();
        env.add_template("record", template)
            .map_err(AppError::internal)?;
        Some(
            env.get_template("record")
                .map_err(AppError::internal)?
                .render(minijinja::value::Value::from_serialize(normalized))
                .map_err(AppError::internal)?,
        )
    } else {
        None
    };

    let now = Utc::now();
    let rrset = if let Some(rrset_id) = existing_rrset_id {
        state
            .rrsets
            .get(&rrset_id)
            .cloned()
            .ok_or_else(|| AppError::internal("rrset disappeared from in-memory storage"))?
    } else {
        let rrset = RecordRrset::restore(
            Uuid::new_v4(),
            record_type.id(),
            record_type.name().clone(),
            DnsClass::IN,
            command.owner_name().clone(),
            command.owner_kind().cloned(),
            anchor_id,
            anchor_name.clone(),
            zone_id,
            command.ttl(),
            now,
            now,
        );
        state.rrsets.insert(rrset.id(), rrset.clone());
        rrset
    };
    let (data, raw_rdata) = match validated {
        ValidatedRecordContent::Structured(data) => (data, None),
        ValidatedRecordContent::RawRdata(raw_rdata) => (Value::Null, Some(raw_rdata)),
    };
    let record = RecordInstance::restore(
        Uuid::new_v4(),
        rrset.id(),
        record_type.id(),
        record_type.name().clone(),
        command.owner_kind().cloned(),
        anchor_id,
        command.owner_name().clone(),
        rrset.zone_id(),
        rrset.ttl(),
        data,
        raw_rdata,
        rendered,
        now,
        now,
    );
    state.records.push(record.clone());
    Ok(record)
}

#[allow(clippy::type_complexity)]
pub(super) fn resolve_record_owner(
    state: &MemoryState,
    owner_kind: Option<&RecordOwnerKind>,
    anchor_name: Option<&str>,
    owner_name: &DnsName,
) -> Result<(Option<Uuid>, Option<String>, Option<Uuid>), AppError> {
    let Some(owner_kind) = owner_kind else {
        return Ok((
            None,
            None,
            best_matching_zone_for_owner_name(state, owner_name),
        ));
    };

    let anchor_name = anchor_name.unwrap_or(owner_name.as_str());
    match owner_kind {
        RecordOwnerKind::Host => {
            let host = state.hosts.get(anchor_name).cloned().ok_or_else(|| {
                AppError::not_found(format!("host '{}' was not found", anchor_name))
            })?;
            let zone_id = host
                .zone()
                .and_then(|zone| state.forward_zones.get(zone.as_str()).map(|zone| zone.id()));
            Ok((
                Some(host.id()),
                Some(host.name().as_str().to_string()),
                zone_id,
            ))
        }
        RecordOwnerKind::ForwardZone => {
            let zone = state
                .forward_zones
                .get(anchor_name)
                .cloned()
                .ok_or_else(|| {
                    AppError::not_found(format!("forward zone '{}' was not found", anchor_name))
                })?;
            Ok((
                Some(zone.id()),
                Some(zone.name().as_str().to_string()),
                Some(zone.id()),
            ))
        }
        RecordOwnerKind::ReverseZone => {
            let zone = state
                .reverse_zones
                .get(anchor_name)
                .cloned()
                .ok_or_else(|| {
                    AppError::not_found(format!("reverse zone '{}' was not found", anchor_name))
                })?;
            Ok((
                Some(zone.id()),
                Some(zone.name().as_str().to_string()),
                Some(zone.id()),
            ))
        }
        RecordOwnerKind::NameServer => {
            let nameserver = state.nameservers.get(anchor_name).cloned().ok_or_else(|| {
                AppError::not_found(format!("nameserver '{}' was not found", anchor_name))
            })?;
            Ok((
                Some(nameserver.id()),
                Some(nameserver.name().as_str().to_string()),
                None,
            ))
        }
        RecordOwnerKind::ForwardZoneDelegation => {
            let delegation = state
                .forward_zone_delegations
                .values()
                .find(|d| d.name().as_str() == anchor_name)
                .cloned()
                .ok_or_else(|| {
                    AppError::not_found(format!(
                        "forward zone delegation '{}' was not found",
                        anchor_name
                    ))
                })?;
            let delegation_name = delegation.name().as_str();
            let owner = owner_name.as_str();
            if owner != delegation_name && !owner.ends_with(&format!(".{}", delegation_name)) {
                return Err(AppError::validation(format!(
                    "owner name '{}' is not within delegation '{}'",
                    owner, delegation_name
                )));
            }
            Ok((
                Some(delegation.id()),
                Some(delegation_name.to_string()),
                Some(delegation.zone_id()),
            ))
        }
        RecordOwnerKind::ReverseZoneDelegation => {
            let delegation = state
                .reverse_zone_delegations
                .values()
                .find(|d| d.name().as_str() == anchor_name)
                .cloned()
                .ok_or_else(|| {
                    AppError::not_found(format!(
                        "reverse zone delegation '{}' was not found",
                        anchor_name
                    ))
                })?;
            let delegation_name = delegation.name().as_str();
            let owner = owner_name.as_str();
            if owner != delegation_name && !owner.ends_with(&format!(".{}", delegation_name)) {
                return Err(AppError::validation(format!(
                    "owner name '{}' is not within delegation '{}'",
                    owner, delegation_name
                )));
            }
            Ok((
                Some(delegation.id()),
                Some(delegation_name.to_string()),
                Some(delegation.zone_id()),
            ))
        }
    }
}

fn best_matching_zone_for_owner_name(state: &MemoryState, owner_name: &DnsName) -> Option<Uuid> {
    state
        .forward_zones
        .values()
        .map(|zone| (zone.id(), zone.name().as_str()))
        .chain(
            state
                .reverse_zones
                .values()
                .map(|zone| (zone.id(), zone.name().as_str())),
        )
        .filter(|(_, zone_name)| {
            owner_name.as_str() == *zone_name
                || owner_name.as_str().ends_with(&format!(".{}", zone_name))
        })
        .max_by_key(|(_, zone_name)| zone_name.len())
        .map(|(zone_id, _)| zone_id)
}

#[async_trait]
impl RecordStore for MemoryStorage {
    async fn list_record_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<RecordTypeDefinition> = state.record_types.values().cloned().collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<RecordRrset> = state.rrsets.values().cloned().collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<RecordInstance> = state
            .records
            .iter()
            .filter(|record| filter.matches(record))
            .cloned()
            .collect();
        sort_items(&mut items, page, |record, field| match field {
            "owner_name" => record.owner_name().to_string(),
            "created_at" => record.created_at().to_rfc3339(),
            _ => record.type_name().as_str().to_string(),
        });
        paginate_by_cursor(items, page)
    }

    async fn create_record_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError> {
        let mut state = self.state.write().await;
        let key = command.name().as_str().to_string();
        if state.record_types.contains_key(&key) {
            return Err(AppError::conflict(format!(
                "record type '{}' already exists",
                key
            )));
        }
        let now = Utc::now();
        let definition = RecordTypeDefinition::restore(
            Uuid::new_v4(),
            command.name().clone(),
            command.dns_type(),
            command.schema().clone(),
            command.built_in(),
            now,
            now,
        );
        state.record_types.insert(key, definition.clone());
        Ok(definition)
    }

    async fn get_record(&self, record_id: Uuid) -> Result<RecordInstance, AppError> {
        let state = self.state.read().await;
        state
            .records
            .iter()
            .find(|r| r.id() == record_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("record not found"))
    }

    async fn get_rrset(&self, rrset_id: Uuid) -> Result<RecordRrset, AppError> {
        let state = self.state.read().await;
        state
            .rrsets
            .get(&rrset_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("rrset not found"))
    }

    async fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError> {
        let mut state = self.state.write().await;
        let record = create_record_in_state(&mut state, command)?;
        if let Some(zone_id) = record.zone_id() {
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        Ok(record)
    }

    async fn update_record(
        &self,
        record_id: Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError> {
        let mut state = self.state.write().await;

        let position = state
            .records
            .iter()
            .position(|r| r.id() == record_id)
            .ok_or_else(|| AppError::not_found("record not found"))?;
        let existing = &state.records[position];

        let record_type = state
            .record_types
            .get(existing.type_name().as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::internal(format!(
                    "record type '{}' not found for existing record",
                    existing.type_name().as_str()
                ))
            })?;

        let new_ttl = match command.ttl() {
            Some(ttl_opt) => ttl_opt,
            None => existing.ttl(),
        };

        let data_changed = command.data().is_some() || command.raw_rdata().is_some();
        let new_data;
        let new_raw_rdata;
        let new_rendered;

        if data_changed {
            let owner_name = DnsName::new(existing.owner_name())?;
            let validated = record_type.validate_record_input(
                &owner_name,
                command.data(),
                command.raw_rdata(),
            )?;

            let same_owner_records = state
                .records
                .iter()
                .filter(|r| r.owner_name() == existing.owner_name() && r.id() != record_id)
                .map(|r| {
                    ExistingRecordSummary::new(
                        r.type_name().clone(),
                        r.ttl(),
                        r.data().clone(),
                        r.raw_rdata().cloned(),
                    )
                })
                .collect::<Vec<_>>();

            let same_rrset_records = state
                .records
                .iter()
                .filter(|r| r.rrset_id() == existing.rrset_id() && r.id() != record_id)
                .map(|r| {
                    ExistingRecordSummary::new(
                        r.type_name().clone(),
                        r.ttl(),
                        r.data().clone(),
                        r.raw_rdata().cloned(),
                    )
                })
                .collect::<Vec<_>>();

            let alias_owner_names = match &validated {
                ValidatedRecordContent::Structured(normalized) => {
                    alias_target_names(normalized, record_type.name())
                }
                ValidatedRecordContent::RawRdata(_) => Vec::new(),
            }
            .into_iter()
            .filter(|target| {
                state
                    .records
                    .iter()
                    .any(|r| r.type_name().as_str() == "CNAME" && r.owner_name() == target.as_str())
            })
            .collect();

            validate_record_relationships(
                &record_type,
                new_ttl,
                &validated,
                &same_owner_records,
                &same_rrset_records,
                &alias_owner_names,
            )?;

            new_rendered = if let (Some(template), ValidatedRecordContent::Structured(normalized)) =
                (record_type.schema().render_template(), &validated)
            {
                let mut env = Environment::new();
                env.add_template("record", template)
                    .map_err(AppError::internal)?;
                Some(
                    env.get_template("record")
                        .map_err(AppError::internal)?
                        .render(minijinja::value::Value::from_serialize(normalized))
                        .map_err(AppError::internal)?,
                )
            } else {
                None
            };

            match validated {
                ValidatedRecordContent::Structured(data) => {
                    new_data = data;
                    new_raw_rdata = None;
                }
                ValidatedRecordContent::RawRdata(raw) => {
                    new_data = Value::Null;
                    new_raw_rdata = Some(raw);
                }
            }
        } else {
            new_data = existing.data().clone();
            new_raw_rdata = existing.raw_rdata().cloned();
            new_rendered = existing.rendered().map(|s| s.to_string());
        }

        let now = Utc::now();
        let updated = RecordInstance::restore(
            existing.id(),
            existing.rrset_id(),
            existing.type_id(),
            existing.type_name().clone(),
            existing.owner_kind().cloned(),
            existing.owner_id(),
            DnsName::new(existing.owner_name())?,
            existing.zone_id(),
            new_ttl,
            new_data,
            new_raw_rdata,
            new_rendered,
            existing.created_at(),
            now,
        );

        state.records[position] = updated.clone();
        if let Some(zone_id) = updated.zone_id() {
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        Ok(updated)
    }

    async fn delete_record(&self, record_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let position = state
            .records
            .iter()
            .position(|r| r.id() == record_id)
            .ok_or_else(|| AppError::not_found("record not found"))?;
        let removed = state.records.remove(position);
        let zone_id = removed.zone_id();
        let rrset_id = removed.rrset_id();
        let rrset_still_has_records = state.records.iter().any(|r| r.rrset_id() == rrset_id);
        if !rrset_still_has_records {
            state.rrsets.remove(&rrset_id);
        }
        if let Some(zone_id) = zone_id {
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        Ok(())
    }

    async fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let key = name.as_str().to_string();
        let record_type = state
            .record_types
            .get(&key)
            .ok_or_else(|| AppError::not_found("record type not found"))?;
        if record_type.built_in() {
            return Err(AppError::conflict("cannot delete built-in record type"));
        }
        let has_records = state
            .records
            .iter()
            .any(|r| r.type_name().as_str().eq_ignore_ascii_case(name.as_str()));
        if has_records {
            return Err(AppError::conflict(
                "cannot delete record type with existing records",
            ));
        }
        state.record_types.remove(&key);
        Ok(())
    }

    async fn delete_rrset(&self, rrset_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let rrset = state
            .rrsets
            .remove(&rrset_id)
            .ok_or_else(|| AppError::not_found("rrset not found"))?;
        state.records.retain(|r| r.rrset_id() != rrset_id);
        if let Some(zone_id) = rrset.zone_id() {
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        Ok(())
    }

    async fn find_records_by_owner(&self, owner_id: Uuid) -> Result<Vec<RecordInstance>, AppError> {
        let state = self.state.read().await;
        let matches: Vec<RecordInstance> = state
            .records
            .iter()
            .filter(|r| r.owner_id() == Some(owner_id))
            .cloned()
            .collect();
        Ok(matches)
    }

    async fn delete_records_by_owner(&self, owner_id: Uuid) -> Result<u64, AppError> {
        let mut state = self.state.write().await;
        let mut removed_records = Vec::new();
        let mut kept = Vec::new();
        for record in state.records.drain(..) {
            if record.owner_id() == Some(owner_id) {
                removed_records.push(record);
            } else {
                kept.push(record);
            }
        }
        state.records = kept;
        let count = removed_records.len() as u64;
        let rrset_ids: HashSet<Uuid> = removed_records.iter().map(|r| r.rrset_id()).collect();
        for rrset_id in rrset_ids {
            if !state.records.iter().any(|r| r.rrset_id() == rrset_id) {
                state.rrsets.remove(&rrset_id);
            }
        }
        Ok(count)
    }

    async fn delete_records_by_owner_name_and_type(
        &self,
        owner_name: &DnsName,
        type_name: &RecordTypeName,
    ) -> Result<u64, AppError> {
        let mut state = self.state.write().await;
        let mut removed_records = Vec::new();
        let mut kept = Vec::new();
        for record in state.records.drain(..) {
            if record
                .owner_name()
                .eq_ignore_ascii_case(owner_name.as_str())
                && record.type_name() == type_name
            {
                removed_records.push(record);
            } else {
                kept.push(record);
            }
        }
        state.records = kept;
        let count = removed_records.len() as u64;
        let rrset_ids: HashSet<Uuid> = removed_records.iter().map(|r| r.rrset_id()).collect();
        for rrset_id in rrset_ids {
            if !state.records.iter().any(|r| r.rrset_id() == rrset_id) {
                state.rrsets.remove(&rrset_id);
            }
        }
        Ok(count)
    }

    async fn rename_record_owner(
        &self,
        owner_id: Uuid,
        new_name: &DnsName,
    ) -> Result<u64, AppError> {
        let mut state = self.state.write().await;
        let mut count: u64 = 0;
        state.records = state
            .records
            .drain(..)
            .map(|r| {
                if r.owner_id() == Some(owner_id) {
                    count += 1;
                    RecordInstance::restore(
                        r.id(),
                        r.rrset_id(),
                        r.type_id(),
                        r.type_name().clone(),
                        r.owner_kind().cloned(),
                        r.owner_id(),
                        new_name.clone(),
                        r.zone_id(),
                        r.ttl(),
                        r.data().clone(),
                        r.raw_rdata().cloned(),
                        r.rendered().map(|s| s.to_string()),
                        r.created_at(),
                        r.updated_at(),
                    )
                } else {
                    r
                }
            })
            .collect();
        // Update rrsets where anchor_id matches
        let rrset_ids: Vec<Uuid> = state
            .rrsets
            .values()
            .filter(|rs| rs.anchor_id() == Some(owner_id))
            .map(|rs| rs.id())
            .collect();
        for rrset_id in rrset_ids {
            if let Some(rrset) = state.rrsets.remove(&rrset_id) {
                let updated = RecordRrset::restore(
                    rrset.id(),
                    rrset.type_id(),
                    rrset.type_name().clone(),
                    rrset.dns_class().clone(),
                    new_name.clone(),
                    rrset.anchor_kind().cloned(),
                    rrset.anchor_id(),
                    Some(new_name.as_str().to_string()),
                    rrset.zone_id(),
                    rrset.ttl(),
                    rrset.created_at(),
                    rrset.updated_at(),
                );
                state.rrsets.insert(rrset_id, updated);
            }
        }
        Ok(count)
    }
}
