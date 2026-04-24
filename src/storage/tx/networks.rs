use crate::{
    domain::{
        filters::NetworkFilter,
        host::IpAddressAssignment,
        network::{CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork},
        pagination::{Page, PageRequest},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::NetworkStore`].
pub trait TxNetworkStore {
    fn list_networks(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError>;
    fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError>;
    fn get_network_by_cidr(&self, cidr: &CidrValue) -> Result<Network, AppError>;
    fn update_network(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError>;
    fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError>;
    fn list_excluded_ranges(
        &self,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError>;
    fn add_excluded_range(
        &self,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError>;
    fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError>;
    fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError>;
    fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError>;
}
