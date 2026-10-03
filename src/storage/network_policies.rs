use async_trait::async_trait;

use crate::{
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{
            CreateNetworkPolicy, CreateNetworkPolicyAttribute, NetworkPolicy,
            NetworkPolicyAttribute, NetworkPolicyAttributeValue, UpdateNetworkPolicy,
            UpdateNetworkPolicyAttribute,
        },
        pagination::{Page, PageRequest},
        types::{NetworkPolicyAttributeName, NetworkPolicyName},
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
    async fn update_network_policy(
        &self,
        name: &NetworkPolicyName,
        command: UpdateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError>;
    async fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError>;
    async fn list_network_policy_attribute_values(
        &self,
        policy: &NetworkPolicyName,
    ) -> Result<Vec<NetworkPolicyAttributeValue>, AppError>;
    async fn list_network_policy_attributes(
        &self,
        page: &PageRequest,
    ) -> Result<Page<NetworkPolicyAttribute>, AppError>;
    async fn create_network_policy_attribute(
        &self,
        command: CreateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError>;
    async fn get_network_policy_attribute_by_name(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<NetworkPolicyAttribute, AppError>;
    async fn update_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
        command: UpdateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError>;
    async fn delete_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<(), AppError>;
}
