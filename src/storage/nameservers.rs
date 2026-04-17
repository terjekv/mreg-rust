use async_trait::async_trait;

use crate::{
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{Page, PageRequest},
        types::DnsName,
    },
    errors::AppError,
};

/// CRUD operations for nameservers.
#[async_trait]
pub trait NameServerStore: Send + Sync {
    async fn list_nameservers(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError>;
    async fn create_nameserver(&self, command: CreateNameServer) -> Result<NameServer, AppError>;
    async fn get_nameserver_by_name(&self, name: &DnsName) -> Result<NameServer, AppError>;
    async fn update_nameserver(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError>;
    async fn delete_nameserver(&self, name: &DnsName) -> Result<(), AppError>;
}
