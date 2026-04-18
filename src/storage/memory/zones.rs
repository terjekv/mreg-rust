use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    domain::{
        pagination::{Page, PageRequest},
        resource_records::{CreateRecordInstance, RecordOwnerKind},
        types::{ZoneName, record_type_names},
        zone::{
            CreateForwardZone, CreateForwardZoneDelegation, CreateReverseZone,
            CreateReverseZoneDelegation, ForwardZone, ForwardZoneDelegation, ReverseZone,
            ReverseZoneDelegation, UpdateForwardZone, UpdateReverseZone,
        },
    },
    errors::AppError,
    storage::ZoneStore,
};

use super::{
    MemoryState, MemoryStorage, bump_zone_serial_in_state,
    delete_records_by_name_and_type_in_state, paginate_by_cursor, records::create_record_in_state,
    sort_and_paginate,
};

pub(super) fn create_forward_zone_in_state(
    state: &mut MemoryState,
    command: CreateForwardZone,
) -> Result<ForwardZone, AppError> {
    let key = command.name().as_str().to_string();
    if state.forward_zones.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "forward zone '{}' already exists",
            key
        )));
    }
    for nameserver in command.nameservers() {
        if !state.nameservers.contains_key(nameserver.as_str()) {
            return Err(AppError::not_found(format!(
                "nameserver '{}' does not exist",
                nameserver.as_str()
            )));
        }
    }
    let now = Utc::now();
    let zone = ForwardZone::restore(
        Uuid::new_v4(),
        command.name().clone(),
        true,
        command.primary_ns().clone(),
        command.nameservers().to_vec(),
        command.email().clone(),
        command.serial_no(),
        now,
        command.refresh(),
        command.retry(),
        command.expire(),
        command.soa_ttl(),
        command.default_ttl(),
        now,
        now,
    )?;
    state.forward_zones.insert(key, zone.clone());
    Ok(zone)
}

pub(super) fn create_reverse_zone_in_state(
    state: &mut MemoryState,
    command: CreateReverseZone,
) -> Result<ReverseZone, AppError> {
    let key = command.name().as_str().to_string();
    if state.reverse_zones.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "reverse zone '{}' already exists",
            key
        )));
    }
    for nameserver in command.nameservers() {
        if !state.nameservers.contains_key(nameserver.as_str()) {
            return Err(AppError::not_found(format!(
                "nameserver '{}' does not exist",
                nameserver.as_str()
            )));
        }
    }
    let now = Utc::now();
    let zone = ReverseZone::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.network().cloned(),
        true,
        command.primary_ns().clone(),
        command.nameservers().to_vec(),
        command.email().clone(),
        command.serial_no(),
        now,
        command.refresh(),
        command.retry(),
        command.expire(),
        command.soa_ttl(),
        command.default_ttl(),
        now,
        now,
    )?;
    state.reverse_zones.insert(key, zone.clone());
    Ok(zone)
}

#[async_trait]
impl ZoneStore for MemoryStorage {
    async fn list_forward_zones(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError> {
        let state = self.state.read().await;
        let items: Vec<ForwardZone> = state.forward_zones.values().cloned().collect();
        sort_and_paginate(items, page, &["created_at"], |zone, field| match field {
            "created_at" => zone.created_at().to_rfc3339(),
            _ => zone.name().as_str().to_string(),
        })
    }

    async fn create_forward_zone(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        let mut state = self.state.write().await;
        let zone = create_forward_zone_in_state(&mut state, command)?;
        // Auto-create NS records for each nameserver
        for ns in zone.nameservers() {
            let cmd = CreateRecordInstance::new(
                record_type_names::ns(),
                RecordOwnerKind::ForwardZone,
                zone.name().as_str(),
                None,
                json!({ "nsdname": ns.as_str() }),
            );
            match cmd {
                Ok(cmd) => {
                    if let Err(err) = create_record_in_state(&mut state, cmd) {
                        tracing::warn!(error = %err, "failed to auto-create cascading NS record");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to construct cascading NS record command");
                }
            }
        }
        Ok(zone)
    }

    async fn get_forward_zone_by_name(&self, name: &ZoneName) -> Result<ForwardZone, AppError> {
        let state = self.state.read().await;
        state
            .forward_zones
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("forward zone '{}' was not found", name.as_str()))
            })
    }

    async fn update_forward_zone(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        let mut state = self.state.write().await;
        let zone = state
            .forward_zones
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("forward zone '{}' was not found", name.as_str()))
            })?;
        let old_nameservers = zone.nameservers().to_vec();
        let now = Utc::now();
        let primary_ns = command
            .primary_ns
            .unwrap_or_else(|| zone.primary_ns().clone());
        let nameservers = command
            .nameservers
            .unwrap_or_else(|| zone.nameservers().to_vec());
        let email = command.email.unwrap_or_else(|| zone.email().clone());
        let refresh = command.refresh.unwrap_or_else(|| zone.refresh());
        let retry = command.retry.unwrap_or_else(|| zone.retry());
        let expire = command.expire.unwrap_or_else(|| zone.expire());
        let soa_ttl = command.soa_ttl.unwrap_or_else(|| zone.soa_ttl());
        let default_ttl = command.default_ttl.unwrap_or_else(|| zone.default_ttl());
        let next_serial = zone.serial_no().next_rfc1912(now.date_naive())?;
        let updated = ForwardZone::restore(
            zone.id(),
            zone.name().clone(),
            true,
            primary_ns,
            nameservers.clone(),
            email,
            next_serial,
            now,
            refresh,
            retry,
            expire,
            soa_ttl,
            default_ttl,
            zone.created_at(),
            now,
        )?;
        state
            .forward_zones
            .insert(name.as_str().to_string(), updated.clone());
        // If nameservers changed, sync NS records
        if old_nameservers != nameservers {
            // Delete old NS records for this zone
            delete_records_by_name_and_type_in_state(&mut state, updated.name().as_str(), "NS");
            // Create new NS records
            for ns in updated.nameservers() {
                let cmd = CreateRecordInstance::new(
                    record_type_names::ns(),
                    RecordOwnerKind::ForwardZone,
                    updated.name().as_str(),
                    None,
                    json!({ "nsdname": ns.as_str() }),
                );
                match cmd {
                    Ok(cmd) => {
                        if let Err(err) = create_record_in_state(&mut state, cmd) {
                            tracing::warn!(error = %err, "failed to auto-create cascading NS record");
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to construct cascading NS record command");
                    }
                }
            }
        }
        Ok(updated)
    }

    async fn delete_forward_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        match state.forward_zones.remove(name.as_str()) {
            Some(_removed) => Ok(()),
            None => Err(AppError::not_found(format!(
                "forward zone '{}' was not found",
                name.as_str()
            ))),
        }
    }

    async fn list_forward_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        let state = self.state.read().await;
        let zone = state.forward_zones.get(zone_name.as_str()).ok_or_else(|| {
            AppError::not_found(format!(
                "forward zone '{}' was not found",
                zone_name.as_str()
            ))
        })?;
        let zone_id = zone.id();
        let mut items: Vec<ForwardZoneDelegation> = state
            .forward_zone_delegations
            .values()
            .filter(|d| d.zone_id() == zone_id)
            .cloned()
            .collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn create_forward_zone_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        let mut state = self.state.write().await;
        let zone = state
            .forward_zones
            .get(command.zone_name().as_str())
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "forward zone '{}' was not found",
                    command.zone_name().as_str()
                ))
            })?;
        let zone_id = zone.id();
        let now = Utc::now();
        let id = Uuid::new_v4();
        let delegation = ForwardZoneDelegation::restore(
            id,
            zone_id,
            command.name().clone(),
            command.comment().to_string(),
            command.nameservers().to_vec(),
            now,
            now,
        )?;
        state
            .forward_zone_delegations
            .insert(id, delegation.clone());
        // Auto-create NS records for the delegation
        for ns in delegation.nameservers() {
            let cmd = CreateRecordInstance::new(
                record_type_names::ns(),
                RecordOwnerKind::ForwardZoneDelegation,
                delegation.name().as_str(),
                None,
                json!({"nsdname": ns.as_str()}),
            );
            match cmd {
                Ok(cmd) => {
                    if let Err(err) = create_record_in_state(&mut state, cmd) {
                        tracing::warn!(error = %err, "failed to auto-create cascading NS record");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to construct cascading NS record command");
                }
            }
        }
        // Bump parent zone serial
        bump_zone_serial_in_state(&mut state, delegation.zone_id());
        Ok(delegation)
    }

    async fn delete_forward_zone_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        // Delete associated records and bump parent zone serial before removing delegation
        if let Some(delegation) = state.forward_zone_delegations.get(&delegation_id) {
            let zone_id = delegation.zone_id();
            let del_name = delegation.name().as_str().to_string();
            delete_records_by_name_and_type_in_state(&mut state, &del_name, "NS");
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        match state.forward_zone_delegations.remove(&delegation_id) {
            Some(_removed) => Ok(()),
            None => Err(AppError::not_found("forward zone delegation was not found")),
        }
    }

    async fn list_reverse_zones(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError> {
        let state = self.state.read().await;
        let items: Vec<ReverseZone> = state.reverse_zones.values().cloned().collect();
        sort_and_paginate(items, page, &["created_at"], |zone, field| match field {
            "created_at" => zone.created_at().to_rfc3339(),
            _ => zone.name().as_str().to_string(),
        })
    }

    async fn create_reverse_zone(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        let mut state = self.state.write().await;
        let zone = create_reverse_zone_in_state(&mut state, command)?;
        // Auto-create NS records for each nameserver
        for ns in zone.nameservers() {
            let cmd = CreateRecordInstance::new(
                record_type_names::ns(),
                RecordOwnerKind::ReverseZone,
                zone.name().as_str(),
                None,
                json!({ "nsdname": ns.as_str() }),
            );
            match cmd {
                Ok(cmd) => {
                    if let Err(err) = create_record_in_state(&mut state, cmd) {
                        tracing::warn!(error = %err, "failed to auto-create cascading NS record");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to construct cascading NS record command");
                }
            }
        }
        Ok(zone)
    }

    async fn get_reverse_zone_by_name(&self, name: &ZoneName) -> Result<ReverseZone, AppError> {
        let state = self.state.read().await;
        state
            .reverse_zones
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("reverse zone '{}' was not found", name.as_str()))
            })
    }

    async fn update_reverse_zone(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        let mut state = self.state.write().await;
        let zone = state
            .reverse_zones
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("reverse zone '{}' was not found", name.as_str()))
            })?;
        let old_nameservers = zone.nameservers().to_vec();
        let now = Utc::now();
        let primary_ns = command
            .primary_ns
            .unwrap_or_else(|| zone.primary_ns().clone());
        let nameservers = command
            .nameservers
            .unwrap_or_else(|| zone.nameservers().to_vec());
        let email = command.email.unwrap_or_else(|| zone.email().clone());
        let refresh = command.refresh.unwrap_or_else(|| zone.refresh());
        let retry = command.retry.unwrap_or_else(|| zone.retry());
        let expire = command.expire.unwrap_or_else(|| zone.expire());
        let soa_ttl = command.soa_ttl.unwrap_or_else(|| zone.soa_ttl());
        let default_ttl = command.default_ttl.unwrap_or_else(|| zone.default_ttl());
        let next_serial = zone.serial_no().next_rfc1912(now.date_naive())?;
        let updated = ReverseZone::restore(
            zone.id(),
            zone.name().clone(),
            zone.network().cloned(),
            true,
            primary_ns,
            nameservers.clone(),
            email,
            next_serial,
            now,
            refresh,
            retry,
            expire,
            soa_ttl,
            default_ttl,
            zone.created_at(),
            now,
        )?;
        state
            .reverse_zones
            .insert(name.as_str().to_string(), updated.clone());
        // If nameservers changed, sync NS records
        if old_nameservers != nameservers {
            // Delete old NS records for this zone
            delete_records_by_name_and_type_in_state(&mut state, updated.name().as_str(), "NS");
            // Create new NS records
            for ns in updated.nameservers() {
                let cmd = CreateRecordInstance::new(
                    record_type_names::ns(),
                    RecordOwnerKind::ReverseZone,
                    updated.name().as_str(),
                    None,
                    json!({ "nsdname": ns.as_str() }),
                );
                match cmd {
                    Ok(cmd) => {
                        if let Err(err) = create_record_in_state(&mut state, cmd) {
                            tracing::warn!(error = %err, "failed to auto-create cascading NS record");
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "failed to construct cascading NS record command");
                    }
                }
            }
        }
        Ok(updated)
    }

    async fn delete_reverse_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        match state.reverse_zones.remove(name.as_str()) {
            Some(_removed) => Ok(()),
            None => Err(AppError::not_found(format!(
                "reverse zone '{}' was not found",
                name.as_str()
            ))),
        }
    }

    async fn list_reverse_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        let state = self.state.read().await;
        let zone = state.reverse_zones.get(zone_name.as_str()).ok_or_else(|| {
            AppError::not_found(format!(
                "reverse zone '{}' was not found",
                zone_name.as_str()
            ))
        })?;
        let zone_id = zone.id();
        let mut items: Vec<ReverseZoneDelegation> = state
            .reverse_zone_delegations
            .values()
            .filter(|d| d.zone_id() == zone_id)
            .cloned()
            .collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn create_reverse_zone_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        let mut state = self.state.write().await;
        let zone = state
            .reverse_zones
            .get(command.zone_name().as_str())
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "reverse zone '{}' was not found",
                    command.zone_name().as_str()
                ))
            })?;
        let zone_id = zone.id();
        let now = Utc::now();
        let id = Uuid::new_v4();
        let delegation = ReverseZoneDelegation::restore(
            id,
            zone_id,
            command.name().clone(),
            command.comment().to_string(),
            command.nameservers().to_vec(),
            now,
            now,
        )?;
        state
            .reverse_zone_delegations
            .insert(id, delegation.clone());
        // Auto-create NS records for the delegation
        for ns in delegation.nameservers() {
            let cmd = CreateRecordInstance::new(
                record_type_names::ns(),
                RecordOwnerKind::ReverseZoneDelegation,
                delegation.name().as_str(),
                None,
                json!({"nsdname": ns.as_str()}),
            );
            match cmd {
                Ok(cmd) => {
                    if let Err(err) = create_record_in_state(&mut state, cmd) {
                        tracing::warn!(error = %err, "failed to auto-create cascading NS record");
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to construct cascading NS record command");
                }
            }
        }
        // Bump parent zone serial
        bump_zone_serial_in_state(&mut state, delegation.zone_id());
        Ok(delegation)
    }

    async fn delete_reverse_zone_delegation(&self, delegation_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        // Delete associated records and bump parent zone serial before removing delegation
        if let Some(delegation) = state.reverse_zone_delegations.get(&delegation_id) {
            let zone_id = delegation.zone_id();
            let del_name = delegation.name().as_str().to_string();
            delete_records_by_name_and_type_in_state(&mut state, &del_name, "NS");
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        match state.reverse_zone_delegations.remove(&delegation_id) {
            Some(_removed) => Ok(()),
            None => Err(AppError::not_found("reverse zone delegation was not found")),
        }
    }

    async fn bump_forward_zone_serial(&self, zone_id: Uuid) -> Result<ForwardZone, AppError> {
        let mut state = self.state.write().await;
        let zone = state
            .forward_zones
            .values_mut()
            .find(|z| z.id() == zone_id)
            .ok_or_else(|| AppError::not_found("forward zone not found"))?;
        let now = Utc::now();
        let next_serial = zone.serial_no().next_rfc1912(now.date_naive())?;
        let updated = ForwardZone::restore(
            zone.id(),
            zone.name().clone(),
            true,
            zone.primary_ns().clone(),
            zone.nameservers().to_vec(),
            zone.email().clone(),
            next_serial,
            now,
            zone.refresh(),
            zone.retry(),
            zone.expire(),
            zone.soa_ttl(),
            zone.default_ttl(),
            zone.created_at(),
            now,
        )?;
        *zone = updated.clone();
        Ok(updated)
    }

    async fn bump_reverse_zone_serial(&self, zone_id: Uuid) -> Result<ReverseZone, AppError> {
        let mut state = self.state.write().await;
        let zone = state
            .reverse_zones
            .values_mut()
            .find(|z| z.id() == zone_id)
            .ok_or_else(|| AppError::not_found("reverse zone not found"))?;
        let now = Utc::now();
        let next_serial = zone.serial_no().next_rfc1912(now.date_naive())?;
        let updated = ReverseZone::restore(
            zone.id(),
            zone.name().clone(),
            zone.network().cloned(),
            true,
            zone.primary_ns().clone(),
            zone.nameservers().to_vec(),
            zone.email().clone(),
            next_serial,
            now,
            zone.refresh(),
            zone.retry(),
            zone.expire(),
            zone.soa_ttl(),
            zone.default_ttl(),
            zone.created_at(),
            now,
        )?;
        *zone = updated.clone();
        Ok(updated)
    }
}
