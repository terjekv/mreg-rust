use async_trait::async_trait;

use crate::{
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest},
        types::{BacnetIdentifier, Hostname},
    },
    errors::AppError,
};

/// CRUD operations for BACnet ID assignments.
#[async_trait]
pub trait BacnetStore: Send + Sync {
    async fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError>;
    async fn create_bacnet_id(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError>;
    async fn get_bacnet_id(
        &self,
        bacnet_id: BacnetIdentifier,
    ) -> Result<BacnetIdAssignment, AppError>;
    async fn list_bacnet_ids_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<BacnetIdAssignment>, AppError>;
    async fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError>;
}
