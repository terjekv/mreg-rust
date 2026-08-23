use async_trait::async_trait;
use chrono::Utc;

use crate::{
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest, paginate_by_key},
        types::{BacnetIdentifier, Hostname},
    },
    errors::AppError,
    storage::BacnetStore,
};

use super::{MemoryState, MemoryStorage};

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

pub(super) fn list_bacnet_ids_in_state(
    state: &MemoryState,
    page: &PageRequest,
    filter: &BacnetIdFilter,
) -> Result<Page<BacnetIdAssignment>, AppError> {
    let mut items: Vec<BacnetIdAssignment> = state
        .bacnet_ids
        .values()
        .filter(|item| filter.matches(item))
        .cloned()
        .collect();
    let sort_by = page.sort_by().unwrap_or("bacnet_id");
    if !["bacnet_id", "host_name", "created_at", "updated_at"].contains(&sort_by) {
        return Err(AppError::validation(format!(
            "unsupported sort_by field for BACnet IDs: {sort_by}"
        )));
    }
    let key = |item: &BacnetIdAssignment| match sort_by {
        "host_name" => item.host_name().as_str().to_string(),
        "created_at" => item.created_at().to_rfc3339(),
        "updated_at" => item.updated_at().to_rfc3339(),
        _ => format!("{:010}", item.bacnet_id().as_u32()),
    };
    items.sort_by(|left, right| {
        let comparison = key(left).cmp(&key(right));
        match page.sort_direction() {
            crate::domain::pagination::SortDirection::Asc => comparison,
            crate::domain::pagination::SortDirection::Desc => comparison.reverse(),
        }
        .then_with(|| left.bacnet_id().as_u32().cmp(&right.bacnet_id().as_u32()))
    });
    paginate_by_key(items, page, sort_by, page.sort_direction(), key, |item| {
        uuid::Uuid::from_u128(u128::from(item.bacnet_id().as_u32()))
    })
}

pub(super) fn get_bacnet_id_in_state(
    state: &MemoryState,
    bacnet_id: BacnetIdentifier,
) -> Result<BacnetIdAssignment, AppError> {
    state
        .bacnet_ids
        .get(&bacnet_id.as_u32())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("bacnet id '{}' was not found", bacnet_id.as_u32()))
        })
}

pub(super) fn list_bacnet_ids_for_hosts_in_state(
    state: &MemoryState,
    hosts: &[Hostname],
) -> Result<Vec<BacnetIdAssignment>, AppError> {
    let host_names = hosts
        .iter()
        .map(|host| host.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    Ok(state
        .bacnet_ids
        .values()
        .filter(|assignment| host_names.contains(assignment.host_name().as_str()))
        .cloned()
        .collect())
}

pub(super) fn delete_bacnet_id_in_state(
    state: &mut MemoryState,
    bacnet_id: BacnetIdentifier,
) -> Result<(), AppError> {
    state
        .bacnet_ids
        .remove(&bacnet_id.as_u32())
        .map(|_| ())
        .ok_or_else(|| {
            AppError::not_found(format!("bacnet id '{}' was not found", bacnet_id.as_u32()))
        })
}

#[async_trait]
impl BacnetStore for MemoryStorage {
    async fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        let state = self.state.read().await;
        list_bacnet_ids_in_state(&state, page, filter)
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
        get_bacnet_id_in_state(&state, bacnet_id)
    }

    async fn list_bacnet_ids_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<BacnetIdAssignment>, AppError> {
        let state = self.state.read().await;
        list_bacnet_ids_for_hosts_in_state(&state, hosts)
    }

    async fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_bacnet_id_in_state(&mut state, bacnet_id)
    }
}
