//! Transaction-scoped, synchronous mirrors of the per-subsystem store traits.
//!
//! See [`crate::storage::Storage::transaction_runner`] and
//! [`crate::storage::DynStorage::transaction`] for how these traits are used.
//! Each `TxXxxStore` mirrors the corresponding async store 1:1, but its methods
//! are synchronous and take `&self` (with interior mutability over a borrowed
//! transaction guard).

mod attachment_community_assignments;
mod attachments;
mod audit;
mod bacnet;
mod communities;
mod host_community_assignments;
mod host_contacts;
mod host_groups;
mod host_policy;
mod hosts;
mod labels;
mod nameservers;
mod network_policies;
mod networks;
mod ptr_overrides;
mod records;
mod zones;

use async_trait::async_trait;

use crate::errors::AppError;

pub use attachment_community_assignments::TxAttachmentCommunityAssignmentStore;
pub use attachments::TxAttachmentStore;
pub use audit::TxAuditStore;
pub use bacnet::TxBacnetStore;
pub use communities::TxCommunityStore;
pub use host_community_assignments::TxHostCommunityAssignmentStore;
pub use host_contacts::TxHostContactStore;
pub use host_groups::TxHostGroupStore;
pub use host_policy::TxHostPolicyStore;
pub use hosts::TxHostStore;
pub use labels::TxLabelStore;
pub use nameservers::TxNameServerStore;
pub use network_policies::TxNetworkPolicyStore;
pub use networks::TxNetworkStore;
pub use ptr_overrides::TxPtrOverrideStore;
pub use records::TxRecordStore;
pub use zones::TxZoneStore;

/// View of the storage backend bound to an in-progress transaction. Exposes
/// every per-subsystem store as a synchronous mirror of the async `Storage`
/// accessors.
pub trait TxStorage {
    fn labels(&self) -> &dyn TxLabelStore;
    fn nameservers(&self) -> &dyn TxNameServerStore;
    fn zones(&self) -> &dyn TxZoneStore;
    fn networks(&self) -> &dyn TxNetworkStore;
    fn hosts(&self) -> &dyn TxHostStore;
    fn attachments(&self) -> &dyn TxAttachmentStore;
    fn attachment_community_assignments(
        &self,
    ) -> &dyn TxAttachmentCommunityAssignmentStore;
    fn host_contacts(&self) -> &dyn TxHostContactStore;
    fn host_groups(&self) -> &dyn TxHostGroupStore;
    fn bacnet(&self) -> &dyn TxBacnetStore;
    fn ptr_overrides(&self) -> &dyn TxPtrOverrideStore;
    fn network_policies(&self) -> &dyn TxNetworkPolicyStore;
    fn communities(&self) -> &dyn TxCommunityStore;
    fn host_community_assignments(&self) -> &dyn TxHostCommunityAssignmentStore;
    fn host_policy(&self) -> &dyn TxHostPolicyStore;
    fn records(&self) -> &dyn TxRecordStore;
    fn audit(&self) -> &dyn TxAuditStore;
}

/// Type-erased transaction body. Used to ferry a generic closure across the
/// `TransactionRunner` trait object boundary while keeping that trait
/// object-safe.
pub trait ErasedTxWork: Send {
    fn run(
        self: Box<Self>,
        tx: &dyn TxStorage,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError>;
}

/// Backend-specific transaction driver. Implemented by each storage backend
/// that supports atomic multi-store mutations. Returned from
/// [`crate::storage::Storage::transaction_runner`].
#[async_trait]
pub trait TransactionRunner: Send + Sync {
    async fn run_transaction(
        &self,
        work: Box<dyn ErasedTxWork>,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError>;
}

/// Adapter that turns a typed `FnOnce(&dyn TxStorage) -> Result<T, AppError>`
/// into an [`ErasedTxWork`] trait object whose result is `Box<dyn Any + Send>`.
pub(crate) struct ClosureTxWork<F, T> {
    work: Option<F>,
    _marker: std::marker::PhantomData<fn() -> T>,
}

impl<F, T> ClosureTxWork<F, T> {
    pub(crate) fn new(work: F) -> Self {
        Self {
            work: Some(work),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<F, T> ErasedTxWork for ClosureTxWork<F, T>
where
    F: FnOnce(&dyn TxStorage) -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    fn run(
        mut self: Box<Self>,
        tx: &dyn TxStorage,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError> {
        let work = self
            .work
            .take()
            .expect("ClosureTxWork::run called more than once");
        let value = work(tx)?;
        Ok(Box::new(value))
    }
}
