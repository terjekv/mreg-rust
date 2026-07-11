use std::collections::HashSet;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{
            CreateNetworkPolicy, CreateNetworkPolicyAttribute, NetworkPolicy,
            NetworkPolicyAttribute, NetworkPolicyAttributeValue, UpdateNetworkPolicy,
            UpdateNetworkPolicyAttribute,
        },
        pagination::{Page, PageRequest},
        types::{NetworkPolicyAttributeName, NetworkPolicyName, UpdateField},
    },
    errors::AppError,
    storage::NetworkPolicyStore,
};

use super::{MemoryState, MemoryStorage, sort_and_paginate};

fn resolve_values(
    state: &MemoryState,
    requested: &[crate::domain::network_policy::SetNetworkPolicyAttributeValue],
) -> Result<Vec<NetworkPolicyAttributeValue>, AppError> {
    let mut seen = HashSet::new();
    requested
        .iter()
        .map(|requested| {
            if !seen.insert(requested.name().clone()) {
                return Err(AppError::validation(format!(
                    "network policy attribute '{}' was provided more than once",
                    requested.name()
                )));
            }
            let attribute = state
                .network_policy_attributes
                .get(requested.name().as_str())
                .ok_or_else(|| {
                    AppError::validation(format!(
                        "network policy attribute '{}' does not exist",
                        requested.name()
                    ))
                })?;
            Ok(NetworkPolicyAttributeValue::restore(
                attribute.id(),
                attribute.name().clone(),
                requested.value(),
            ))
        })
        .collect()
}

pub(super) fn create_network_policy_in_state(
    state: &mut MemoryState,
    command: CreateNetworkPolicy,
) -> Result<NetworkPolicy, AppError> {
    let key = command.name().as_str().to_string();
    if state.network_policies.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "network policy '{}' already exists",
            key
        )));
    }
    let values = resolve_values(state, command.attributes())?;
    if let Some(pattern) = command.community_template_pattern()
        && state
            .network_policies
            .values()
            .any(|policy| policy.community_template_pattern() == Some(pattern))
    {
        return Err(AppError::conflict(
            "community template pattern already exists",
        ));
    }
    let now = Utc::now();
    let policy = NetworkPolicy::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description(),
        command.community_template_pattern().map(str::to_string),
        now,
        now,
    )?;
    state.network_policies.insert(key, policy.clone());
    state
        .network_policy_attribute_values
        .insert(policy.id(), values);
    Ok(policy)
}

pub(super) fn list_network_policies_in_state(
    state: &MemoryState,
    page: &PageRequest,
    filter: &NetworkPolicyFilter,
) -> Result<Page<NetworkPolicy>, AppError> {
    let items = state
        .network_policies
        .values()
        .filter(|policy| filter.matches(policy))
        .cloned()
        .collect();
    sort_and_paginate(
        items,
        page,
        &["description", "created_at", "updated_at"],
        |policy, field| match field {
            "description" => policy.description().to_string(),
            "created_at" => policy.created_at().to_rfc3339(),
            "updated_at" => policy.updated_at().to_rfc3339(),
            _ => policy.name().as_str().to_string(),
        },
    )
}

pub(super) fn get_network_policy_by_name_in_state(
    state: &MemoryState,
    name: &NetworkPolicyName,
) -> Result<NetworkPolicy, AppError> {
    state
        .network_policies
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("network policy '{}' was not found", name.as_str()))
        })
}

pub(super) fn update_network_policy_in_state(
    state: &mut MemoryState,
    name: &NetworkPolicyName,
    command: UpdateNetworkPolicy,
) -> Result<NetworkPolicy, AppError> {
    let old = get_network_policy_by_name_in_state(state, name)?;
    let new_name = command.name.unwrap_or_else(|| old.name().clone());
    if new_name != *name && state.network_policies.contains_key(new_name.as_str()) {
        return Err(AppError::conflict(format!(
            "network policy '{}' already exists",
            new_name
        )));
    }
    let pattern = match command.community_template_pattern {
        UpdateField::Unchanged => old.community_template_pattern().map(str::to_string),
        UpdateField::Clear => None,
        UpdateField::Set(value) => Some(value),
    };
    if state.network_policies.values().any(|policy| {
        policy.id() != old.id()
            && pattern.is_some()
            && policy.community_template_pattern() == pattern.as_deref()
    }) {
        return Err(AppError::conflict(
            "community template pattern already exists",
        ));
    }
    let replacement = command
        .attributes
        .as_deref()
        .map(|values| resolve_values(state, values))
        .transpose()?;
    let updated = NetworkPolicy::restore(
        old.id(),
        new_name.clone(),
        command
            .description
            .unwrap_or_else(|| old.description().to_string()),
        pattern,
        old.created_at(),
        Utc::now(),
    )?;
    if let Some(values) = replacement {
        state
            .network_policy_attribute_values
            .insert(old.id(), values);
    }
    state.network_policies.remove(name.as_str());
    state
        .network_policies
        .insert(new_name.as_str().to_string(), updated.clone());
    Ok(updated)
}

pub(super) fn delete_network_policy_in_state(
    state: &mut MemoryState,
    name: &NetworkPolicyName,
) -> Result<(), AppError> {
    let policy = state
        .network_policies
        .remove(name.as_str())
        .ok_or_else(|| {
            AppError::not_found(format!("network policy '{}' was not found", name.as_str()))
        })?;
    state.network_policy_attribute_values.remove(&policy.id());
    Ok(())
}

pub(super) fn list_network_policy_attribute_values_in_state(
    state: &MemoryState,
    policy: &NetworkPolicyName,
) -> Result<Vec<NetworkPolicyAttributeValue>, AppError> {
    let policy = get_network_policy_by_name_in_state(state, policy)?;
    Ok(state
        .network_policy_attribute_values
        .get(&policy.id())
        .cloned()
        .unwrap_or_default())
}

pub(super) fn list_network_policy_attributes_in_state(
    state: &MemoryState,
    page: &PageRequest,
) -> Result<Page<NetworkPolicyAttribute>, AppError> {
    let mut page = page.clone();
    if page.sort_by.is_none() {
        page.sort_by = Some("created_at".to_string());
    }
    sort_and_paginate(
        state.network_policy_attributes.values().cloned().collect(),
        &page,
        &["description", "created_at", "updated_at"],
        |attribute, field| match field {
            "description" => attribute.description().to_string(),
            "created_at" => attribute.created_at().to_rfc3339(),
            "updated_at" => attribute.updated_at().to_rfc3339(),
            _ => attribute.name().as_str().to_string(),
        },
    )
}

pub(super) fn create_network_policy_attribute_in_state(
    state: &mut MemoryState,
    command: CreateNetworkPolicyAttribute,
) -> Result<NetworkPolicyAttribute, AppError> {
    let key = command.name().as_str().to_string();
    if state.network_policy_attributes.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "network policy attribute '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let attribute = NetworkPolicyAttribute::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description(),
        now,
        now,
    );
    state
        .network_policy_attributes
        .insert(key, attribute.clone());
    Ok(attribute)
}

pub(super) fn get_network_policy_attribute_by_name_in_state(
    state: &MemoryState,
    name: &NetworkPolicyAttributeName,
) -> Result<NetworkPolicyAttribute, AppError> {
    state
        .network_policy_attributes
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("network policy attribute '{}' was not found", name))
        })
}

pub(super) fn update_network_policy_attribute_in_state(
    state: &mut MemoryState,
    name: &NetworkPolicyAttributeName,
    command: UpdateNetworkPolicyAttribute,
) -> Result<NetworkPolicyAttribute, AppError> {
    let old = get_network_policy_attribute_by_name_in_state(state, name)?;
    let new_name = command.name.unwrap_or_else(|| old.name().clone());
    if new_name != *name
        && state
            .network_policy_attributes
            .contains_key(new_name.as_str())
    {
        return Err(AppError::conflict(format!(
            "network policy attribute '{}' already exists",
            new_name
        )));
    }
    let updated = NetworkPolicyAttribute::restore(
        old.id(),
        new_name.clone(),
        command
            .description
            .unwrap_or_else(|| old.description().to_string()),
        old.created_at(),
        Utc::now(),
    );
    for values in state.network_policy_attribute_values.values_mut() {
        for value in values
            .iter_mut()
            .filter(|value| value.attribute_id() == old.id())
        {
            *value =
                NetworkPolicyAttributeValue::restore(old.id(), new_name.clone(), value.value());
        }
    }
    state.network_policy_attributes.remove(name.as_str());
    state
        .network_policy_attributes
        .insert(new_name.as_str().to_string(), updated.clone());
    Ok(updated)
}

pub(super) fn delete_network_policy_attribute_in_state(
    state: &mut MemoryState,
    name: &NetworkPolicyAttributeName,
) -> Result<(), AppError> {
    let attribute = state
        .network_policy_attributes
        .remove(name.as_str())
        .ok_or_else(|| {
            AppError::not_found(format!("network policy attribute '{}' was not found", name))
        })?;
    for values in state.network_policy_attribute_values.values_mut() {
        values.retain(|value| value.attribute_id() != attribute.id());
    }
    Ok(())
}

#[async_trait]
impl NetworkPolicyStore for MemoryStorage {
    async fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        let state = self.state.read().await;
        list_network_policies_in_state(&state, page, filter)
    }

    async fn create_network_policy(
        &self,
        command: CreateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError> {
        let mut state = self.state.write().await;
        create_network_policy_in_state(&mut state, command)
    }

    async fn get_network_policy_by_name(
        &self,
        name: &NetworkPolicyName,
    ) -> Result<NetworkPolicy, AppError> {
        let state = self.state.read().await;
        get_network_policy_by_name_in_state(&state, name)
    }

    async fn update_network_policy(
        &self,
        name: &NetworkPolicyName,
        command: UpdateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError> {
        let mut state = self.state.write().await;
        update_network_policy_in_state(&mut state, name, command)
    }

    async fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_network_policy_in_state(&mut state, name)
    }

    async fn list_network_policy_attribute_values(
        &self,
        policy: &NetworkPolicyName,
    ) -> Result<Vec<NetworkPolicyAttributeValue>, AppError> {
        let state = self.state.read().await;
        list_network_policy_attribute_values_in_state(&state, policy)
    }

    async fn list_network_policy_attributes(
        &self,
        page: &PageRequest,
    ) -> Result<Page<NetworkPolicyAttribute>, AppError> {
        let state = self.state.read().await;
        list_network_policy_attributes_in_state(&state, page)
    }

    async fn create_network_policy_attribute(
        &self,
        command: CreateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError> {
        let mut state = self.state.write().await;
        create_network_policy_attribute_in_state(&mut state, command)
    }

    async fn get_network_policy_attribute_by_name(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<NetworkPolicyAttribute, AppError> {
        let state = self.state.read().await;
        get_network_policy_attribute_by_name_in_state(&state, name)
    }

    async fn update_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
        command: UpdateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError> {
        let mut state = self.state.write().await;
        update_network_policy_attribute_in_state(&mut state, name, command)
    }

    async fn delete_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_network_policy_attribute_in_state(&mut state, name)
    }
}
