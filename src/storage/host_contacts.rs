use async_trait::async_trait;

use crate::{
    domain::{
        filters::HostContactFilter,
        host_contact::{CreateHostContact, HostContact},
        pagination::{Page, PageRequest},
        types::{EmailAddressValue, Hostname},
    },
    errors::AppError,
};

/// CRUD operations for host contacts.
#[async_trait]
pub trait HostContactStore: Send + Sync {
    async fn list_host_contacts(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError>;
    async fn create_host_contact(
        &self,
        command: CreateHostContact,
    ) -> Result<HostContact, AppError>;
    async fn get_host_contact_by_email(
        &self,
        email: &EmailAddressValue,
    ) -> Result<HostContact, AppError>;
    async fn list_host_contacts_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostContact>, AppError>;
    async fn delete_host_contact(&self, email: &EmailAddressValue) -> Result<(), AppError>;
}
