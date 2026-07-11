use crate::{
    domain::{
        filters::HostContactFilter,
        host_contact::{CreateHostContact, HostContact},
        pagination::{Page, PageRequest},
        types::{EmailAddressValue, Hostname},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::HostContactStore`].
pub trait TxHostContactStore {
    fn list_host_contacts(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError>;
    fn create_host_contact(&self, command: CreateHostContact) -> Result<HostContact, AppError>;
    fn get_host_contact_by_email(
        &self,
        email: &EmailAddressValue,
    ) -> Result<HostContact, AppError>;
    fn list_host_contacts_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostContact>, AppError>;
    fn delete_host_contact(&self, email: &EmailAddressValue) -> Result<(), AppError>;
}
