use async_trait::async_trait;

use crate::{
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{CreateNetworkPolicy, NetworkPolicy},
        pagination::{Page, PageRequest},
        types::NetworkPolicyName,
    },
    errors::AppError,
};

/// CRUD operations for network policies.
#[async_trait]
pub trait NetworkPolicyStore: Send + Sync {
    async fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError>;
    async fn create_network_policy(
        &self,
        command: CreateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError>;
    async fn get_network_policy_by_name(
        &self,
        name: &NetworkPolicyName,
    ) -> Result<NetworkPolicy, AppError>;
    async fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError>;
}
