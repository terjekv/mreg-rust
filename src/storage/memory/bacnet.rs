use async_trait::async_trait;
use chrono::Utc;

use crate::{
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest},
        types::BacnetIdentifier,
    },
    errors::AppError,
    storage::BacnetStore,
};

use super::{MemoryState, MemoryStorage, paginate_simple};

pub(super) fn create_bacnet_id_in_state(
    state: &mut MemoryState,
    command: CreateBacnetIdAssignment,
) -> Result<BacnetIdAssignment, AppError> {
    if !state.hosts.contains_key(command.host_name().as_str()) {
        return Err(AppError::not_found(format!(
            "host '{}' was not found",
            command.host_name().as_str()
        )));
    }
    if state.bacnet_ids.contains_key(&command.bacnet_id().as_u32()) {
        return Err(AppError::conflict(format!(
            "bacnet id '{}' already exists",
            command.bacnet_id().as_u32()
        )));
    }
    if state
        .bacnet_ids
        .values()
        .any(|assignment| assignment.host_name() == command.host_name())
    {
        return Err(AppError::conflict(format!(
            "host '{}' already has a bacnet id",
            command.host_name().as_str()
        )));
    }
    let now = Utc::now();
    let assignment =
        BacnetIdAssignment::restore(command.bacnet_id(), command.host_name().clone(), now, now);
    state
        .bacnet_ids
        .insert(command.bacnet_id().as_u32(), assignment.clone());
    Ok(assignment)
}

#[async_trait]
impl BacnetStore for MemoryStorage {
    async fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        let state = self.state.read().await;
        let items: Vec<BacnetIdAssignment> = state
            .bacnet_ids
            .values()
            .filter(|item| filter.matches(item))
            .cloned()
            .collect();
        Ok(paginate_simple(items, page))
    }

    async fn create_bacnet_id(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError> {
        let mut state = self.state.write().await;
        create_bacnet_id_in_state(&mut state, command)
    }

    async fn get_bacnet_id(
        &self,
        bacnet_id: BacnetIdentifier,
    ) -> Result<BacnetIdAssignment, AppError> {
        let state = self.state.read().await;
        state
            .bacnet_ids
            .get(&bacnet_id.as_u32())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("bacnet id '{}' was not found", bacnet_id.as_u32()))
            })
    }

    async fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .bacnet_ids
            .remove(&bacnet_id.as_u32())
            .map(|_| ())
            .ok_or_else(|| {
                AppError::not_found(format!("bacnet id '{}' was not found", bacnet_id.as_u32()))
            })
    }
}
