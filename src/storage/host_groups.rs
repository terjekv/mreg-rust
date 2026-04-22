use async_trait::async_trait;

use crate::{
    domain::{
        filters::HostGroupFilter,
        host_group::{CreateHostGroup, HostGroup},
        pagination::{Page, PageRequest},
        types::{HostGroupName, Hostname},
    },
    errors::AppError,
};

/// CRUD operations for host groups.
#[async_trait]
pub trait HostGroupStore: Send + Sync {
    async fn list_host_groups(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError>;
    async fn create_host_group(&self, command: CreateHostGroup) -> Result<HostGroup, AppError>;
    async fn get_host_group_by_name(&self, name: &HostGroupName) -> Result<HostGroup, AppError>;
    async fn list_host_groups_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostGroup>, AppError>;
    async fn delete_host_group(&self, name: &HostGroupName) -> Result<(), AppError>;
}
