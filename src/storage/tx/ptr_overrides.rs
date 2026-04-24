use crate::{
    domain::{
        filters::PtrOverrideFilter,
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        types::IpAddressValue,
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::PtrOverrideStore`].
pub trait TxPtrOverrideStore {
    fn list_ptr_overrides(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError>;
    fn create_ptr_override(
        &self,
        command: CreatePtrOverride,
    ) -> Result<PtrOverride, AppError>;
    fn get_ptr_override_by_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<PtrOverride, AppError>;
    fn delete_ptr_override(&self, address: &IpAddressValue) -> Result<(), AppError>;
}
