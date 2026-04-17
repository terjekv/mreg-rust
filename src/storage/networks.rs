use async_trait::async_trait;

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

/// CRUD operations for networks and excluded IP ranges.
#[async_trait]
pub trait NetworkStore: Send + Sync {
    async fn list_networks(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError>;
    async fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError>;
    async fn get_network_by_cidr(&self, cidr: &CidrValue) -> Result<Network, AppError>;
    async fn update_network(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError>;
    async fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError>;
    async fn list_excluded_ranges(
        &self,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError>;
    async fn add_excluded_range(
        &self,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError>;
    async fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError>;
    async fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError>;
    async fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError>;
}
