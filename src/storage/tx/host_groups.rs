use crate::{
    domain::{
        filters::HostGroupFilter,
        host_group::{CreateHostGroup, HostGroup},
        pagination::{Page, PageRequest},
        types::{HostGroupName, Hostname},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::HostGroupStore`].
pub trait TxHostGroupStore {
    fn list_host_groups(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError>;
    fn create_host_group(&self, command: CreateHostGroup) -> Result<HostGroup, AppError>;
    fn get_host_group_by_name(&self, name: &HostGroupName) -> Result<HostGroup, AppError>;
    fn list_host_groups_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostGroup>, AppError>;
    fn delete_host_group(&self, name: &HostGroupName) -> Result<(), AppError>;
}
