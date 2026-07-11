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

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::NetworkPolicyStore`].
pub trait TxNetworkPolicyStore {
    fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError>;
    fn create_network_policy(
        &self,
        command: CreateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError>;
    fn get_network_policy_by_name(
        &self,
        name: &NetworkPolicyName,
    ) -> Result<NetworkPolicy, AppError>;
    fn update_network_policy(
        &self,
        name: &NetworkPolicyName,
        command: UpdateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError>;
    fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError>;
    fn list_network_policy_attribute_values(
        &self,
        policy: &NetworkPolicyName,
    ) -> Result<Vec<NetworkPolicyAttributeValue>, AppError>;
    fn list_network_policy_attributes(
        &self,
        page: &PageRequest,
    ) -> Result<Page<NetworkPolicyAttribute>, AppError>;
    fn create_network_policy_attribute(
        &self,
        command: CreateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError>;
    fn get_network_policy_attribute_by_name(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<NetworkPolicyAttribute, AppError>;
    fn update_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
        command: UpdateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError>;
    fn delete_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<(), AppError>;
}
