use crate::{
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{Page, PageRequest},
        types::DnsName,
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::NameServerStore`].
pub trait TxNameServerStore {
    fn list_nameservers(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError>;
    fn create_nameserver(&self, command: CreateNameServer) -> Result<NameServer, AppError>;
    fn get_nameserver_by_name(&self, name: &DnsName) -> Result<NameServer, AppError>;
    fn update_nameserver(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError>;
    fn delete_nameserver(&self, name: &DnsName) -> Result<(), AppError>;
}
