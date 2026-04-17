use async_trait::async_trait;

use crate::{
    domain::{
        filters::PtrOverrideFilter,
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        types::IpAddressValue,
    },
    errors::AppError,
};

/// CRUD operations for PTR overrides.
#[async_trait]
pub trait PtrOverrideStore: Send + Sync {
    async fn list_ptr_overrides(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError>;
    async fn create_ptr_override(
        &self,
        command: CreatePtrOverride,
    ) -> Result<PtrOverride, AppError>;
    async fn get_ptr_override_by_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<PtrOverride, AppError>;
    async fn delete_ptr_override(&self, address: &IpAddressValue) -> Result<(), AppError>;
}
