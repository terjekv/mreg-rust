use crate::{
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest},
        types::{BacnetIdentifier, Hostname},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::BacnetStore`].
pub trait TxBacnetStore {
    fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError>;
    fn create_bacnet_id(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError>;
    fn get_bacnet_id(
        &self,
        bacnet_id: BacnetIdentifier,
    ) -> Result<BacnetIdAssignment, AppError>;
    fn list_bacnet_ids_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<BacnetIdAssignment>, AppError>;
    fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError>;
}
