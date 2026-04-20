use async_trait::async_trait;

use crate::{
    domain::{
        filters::HostFilter,
        host::{
            AssignIpAddress, CreateHost, Host, HostAuthContext, IpAddressAssignment, UpdateHost,
            UpdateIpAddress,
        },
        pagination::{Page, PageRequest},
        types::{Hostname, IpAddressValue},
    },
    errors::AppError,
};

/// CRUD operations for hosts and their IP address assignments.
#[async_trait]
pub trait HostStore: Send + Sync {
    async fn list_hosts(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError>;
    async fn create_host(&self, command: CreateHost) -> Result<Host, AppError>;
    async fn get_host_by_name(&self, name: &Hostname) -> Result<Host, AppError>;
    async fn list_hosts_by_names(&self, names: &[Hostname]) -> Result<Vec<Host>, AppError>;
    async fn get_host_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError>;
    async fn update_host(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError>;
    async fn delete_host(&self, name: &Hostname) -> Result<(), AppError>;
    async fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError>;
    async fn list_ip_addresses_for_host(
        &self,
        host: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError>;
    async fn list_ip_addresses_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<IpAddressAssignment>, AppError>;
    async fn get_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError>;
    async fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError>;
    async fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError>;
    async fn unassign_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError>;
}
