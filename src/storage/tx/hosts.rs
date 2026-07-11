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

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::HostStore`].
///
/// Methods take `&self` and use interior mutability to acquire the underlying
/// connection or state guard. No `Send`/`Sync` bound: the trait object lives
/// only for the duration of the transaction closure, which runs single-threaded
/// inside one `spawn_blocking` worker (Postgres) or under the write lock
/// (Memory).
pub trait TxHostStore {
    fn list_hosts(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError>;
    fn create_host(&self, command: CreateHost) -> Result<Host, AppError>;
    fn get_host_by_name(&self, name: &Hostname) -> Result<Host, AppError>;
    fn list_hosts_by_names(&self, names: &[Hostname]) -> Result<Vec<Host>, AppError>;
    fn get_host_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError>;
    fn update_host(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError>;
    fn delete_host(&self, name: &Hostname) -> Result<(), AppError>;
    fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError>;
    fn list_ip_addresses_for_host(
        &self,
        host: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError>;
    fn list_ip_addresses_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<IpAddressAssignment>, AppError>;
    fn get_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError>;
    fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError>;
    fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError>;
    fn unassign_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError>;
}
