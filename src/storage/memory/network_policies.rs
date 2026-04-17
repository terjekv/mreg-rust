use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{CreateNetworkPolicy, NetworkPolicy},
        pagination::{Page, PageRequest},
        types::NetworkPolicyName,
    },
    errors::AppError,
    storage::NetworkPolicyStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

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
    let now = Utc::now();
    let policy = NetworkPolicy::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description().to_string(),
        command.community_template_pattern().map(str::to_string),
        now,
        now,
    )?;
    state.network_policies.insert(key, policy.clone());
    Ok(policy)
}

#[async_trait]
impl NetworkPolicyStore for MemoryStorage {
    async fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<NetworkPolicy> = state
            .network_policies
            .values()
            .filter(|policy| filter.matches(policy))
            .cloned()
            .collect();
        sort_items(
            &mut items,
            page,
            &["description", "created_at", "updated_at"],
            |policy, field| match field {
                "description" => policy.description().to_string(),
                "created_at" => policy.created_at().to_rfc3339(),
                "updated_at" => policy.updated_at().to_rfc3339(),
                _ => policy.name().as_str().to_string(),
            },
        )?;
        paginate_by_cursor(items, page)
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
        state
            .network_policies
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("network policy '{}' was not found", name.as_str()))
            })
    }

    async fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .network_policies
            .remove(name.as_str())
            .map(|_| ())
            .ok_or_else(|| {
                AppError::not_found(format!("network policy '{}' was not found", name.as_str()))
            })
    }
}
