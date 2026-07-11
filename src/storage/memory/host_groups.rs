use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::HostGroupFilter,
        host_group::{CreateHostGroup, HostGroup},
        pagination::{Page, PageRequest},
        types::{HostGroupName, Hostname},
    },
    errors::AppError,
    storage::HostGroupStore,
};

use super::{MemoryState, MemoryStorage, sort_and_paginate};

pub(super) fn create_host_group_in_state(
    state: &mut MemoryState,
    command: CreateHostGroup,
) -> Result<HostGroup, AppError> {
    let key = command.name().as_str().to_string();
    if state.host_groups.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "host group '{}' already exists",
            key
        )));
    }
    for host in command.hosts() {
        if !state.hosts.contains_key(host.as_str()) {
            return Err(AppError::not_found(format!(
                "host '{}' was not found",
                host.as_str()
            )));
        }
    }
    for parent in command.parent_groups() {
        if parent == command.name() {
            return Err(AppError::validation("host group cannot be its own parent"));
        }
        if !state.host_groups.contains_key(parent.as_str()) {
            return Err(AppError::not_found(format!(
                "host group '{}' was not found",
                parent.as_str()
            )));
        }
    }
    let now = Utc::now();
    let group = HostGroup::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description().to_string(),
        command.hosts().to_vec(),
        command.parent_groups().to_vec(),
        command.owner_groups().to_vec(),
        now,
        now,
    )?;
    state.host_groups.insert(key, group.clone());
    Ok(group)
}

pub(super) fn list_host_groups_in_state(
    state: &MemoryState,
    page: &PageRequest,
    filter: &HostGroupFilter,
) -> Result<Page<HostGroup>, AppError> {
    let items: Vec<HostGroup> = state
        .host_groups
        .values()
        .filter(|group| filter.matches(group))
        .cloned()
        .collect();
    sort_and_paginate(
        items,
        page,
        &["description", "created_at", "updated_at"],
        |group, field| match field {
            "description" => group.description().to_string(),
            "created_at" => group.created_at().to_rfc3339(),
            "updated_at" => group.updated_at().to_rfc3339(),
            _ => group.name().as_str().to_string(),
        },
    )
}

pub(super) fn get_host_group_by_name_in_state(
    state: &MemoryState,
    name: &HostGroupName,
) -> Result<HostGroup, AppError> {
    state
        .host_groups
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("host group '{}' was not found", name.as_str()))
        })
}

pub(super) fn list_host_groups_for_hosts_in_state(
    state: &MemoryState,
    hosts: &[Hostname],
) -> Result<Vec<HostGroup>, AppError> {
    let host_names = hosts
        .iter()
        .map(|host| host.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    Ok(state
        .host_groups
        .values()
        .filter(|group| {
            group
                .hosts()
                .iter()
                .any(|host| host_names.contains(host.as_str()))
        })
        .cloned()
        .collect())
}

pub(super) fn delete_host_group_in_state(
    state: &mut MemoryState,
    name: &HostGroupName,
) -> Result<(), AppError> {
    state
        .host_groups
        .remove(name.as_str())
        .map(|_| ())
        .ok_or_else(|| {
            AppError::not_found(format!("host group '{}' was not found", name.as_str()))
        })
}

#[async_trait]
impl HostGroupStore for MemoryStorage {
    async fn list_host_groups(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError> {
        let state = self.state.read().await;
        list_host_groups_in_state(&state, page, filter)
    }

    async fn create_host_group(&self, command: CreateHostGroup) -> Result<HostGroup, AppError> {
        let mut state = self.state.write().await;
        create_host_group_in_state(&mut state, command)
    }

    async fn get_host_group_by_name(&self, name: &HostGroupName) -> Result<HostGroup, AppError> {
        let state = self.state.read().await;
        get_host_group_by_name_in_state(&state, name)
    }

    async fn list_host_groups_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostGroup>, AppError> {
        let state = self.state.read().await;
        list_host_groups_for_hosts_in_state(&state, hosts)
    }

    async fn delete_host_group(&self, name: &HostGroupName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_host_group_in_state(&mut state, name)
    }
}
