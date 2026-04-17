use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::PtrOverrideFilter,
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        types::IpAddressValue,
    },
    errors::AppError,
    storage::PtrOverrideStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

pub(super) fn create_ptr_override_in_state(
    state: &mut MemoryState,
    command: CreatePtrOverride,
) -> Result<PtrOverride, AppError> {
    let host = state
        .hosts
        .get(command.host_name().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host '{}' was not found",
                command.host_name().as_str()
            ))
        })?;
    let assignment = state
        .ip_addresses
        .get(&command.address().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "ip address '{}' was not found",
                command.address().as_str()
            ))
        })?;
    if assignment.host_id() != host.id() {
        return Err(AppError::validation(
            "PTR override address must belong to the supplied host",
        ));
    }
    let key = command.address().as_str();
    if state.ptr_overrides.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "ptr override '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let override_record = PtrOverride::restore(
        Uuid::new_v4(),
        command.host_name().clone(),
        *command.address(),
        command.target_name().cloned(),
        now,
        now,
    );
    state.ptr_overrides.insert(key, override_record.clone());
    Ok(override_record)
}

#[async_trait]
impl PtrOverrideStore for MemoryStorage {
    async fn list_ptr_overrides(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<PtrOverride> = state
            .ptr_overrides
            .values()
            .filter(|ptr| filter.matches(ptr))
            .cloned()
            .collect();
        sort_items(
            &mut items,
            page,
            &["address", "created_at"],
            |ptr, field| match field {
                "address" => ptr.address().as_str(),
                "created_at" => ptr.created_at().to_rfc3339(),
                _ => ptr.host_name().as_str().to_string(),
            },
        )?;
        paginate_by_cursor(items, page)
    }

    async fn create_ptr_override(
        &self,
        command: CreatePtrOverride,
    ) -> Result<PtrOverride, AppError> {
        let mut state = self.state.write().await;
        create_ptr_override_in_state(&mut state, command)
    }

    async fn get_ptr_override_by_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<PtrOverride, AppError> {
        let state = self.state.read().await;
        state
            .ptr_overrides
            .get(&address.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("ptr override '{}' was not found", address.as_str()))
            })
    }

    async fn delete_ptr_override(&self, address: &IpAddressValue) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .ptr_overrides
            .remove(&address.as_str())
            .map(|_| ())
            .ok_or_else(|| {
                AppError::not_found(format!("ptr override '{}' was not found", address.as_str()))
            })
    }
}
