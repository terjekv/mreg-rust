// Backend modules
pub mod memory;
pub mod postgres;
pub mod readable;

// Shared cross-backend helpers
pub(crate) mod has_id;

// Shared import helpers (used by both backends)
pub(crate) mod import_helpers;

// Transaction-scoped sub-store traits
pub mod tx;

// Trait definition modules
mod attachment_community_assignments;
mod attachments;
mod audit_store;
mod auth_sessions;
mod bacnet;
mod communities;
mod exports_store;
mod host_community_assignments;
mod host_contacts;
mod host_groups;
mod host_policy_store;
mod host_views_store;
mod hosts;
mod imports_store;
mod labels;
mod nameservers;
mod network_policies;
mod networks;
mod ptr_overrides;
mod records;
mod tasks;
mod zones;

// Re-export all traits
pub use attachment_community_assignments::AttachmentCommunityAssignmentStore;
pub use attachments::AttachmentStore;
pub use audit_store::AuditStore;
pub use auth_sessions::AuthSessionStore;
pub use bacnet::BacnetStore;
pub use communities::CommunityStore;
pub use exports_store::ExportStore;
pub use host_community_assignments::HostCommunityAssignmentStore;
pub use host_contacts::HostContactStore;
pub use host_groups::HostGroupStore;
pub use host_policy_store::HostPolicyStore;
pub use host_views_store::HostViewStore;
pub use hosts::HostStore;
pub use imports_store::ImportStore;
pub use labels::LabelStore;
pub use nameservers::NameServerStore;
pub use network_policies::NetworkPolicyStore;
pub use networks::NetworkStore;
pub use ptr_overrides::PtrOverrideStore;
pub use records::RecordStore;
pub use tasks::TaskStore;
pub use zones::ZoneStore;

pub use readable::ReadableStorage;

pub use tx::{
    ErasedTxWork, TransactionRunner, TxAttachmentCommunityAssignmentStore, TxAttachmentStore,
    TxAuditStore, TxBacnetStore, TxCommunityStore, TxHostCommunityAssignmentStore,
    TxHostContactStore, TxHostGroupStore, TxHostPolicyStore, TxHostStore, TxLabelStore,
    TxNameServerStore, TxNetworkPolicyStore, TxNetworkStore, TxPtrOverrideStore, TxRecordStore,
    TxStorage, TxZoneStore,
};

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use crate::{
    config::{Config, StorageBackendSetting},
    db::Database,
    errors::AppError,
};

/// Thread-safe shared storage handle used across HTTP handlers.
///
/// Wraps `Arc<dyn Storage>` as a thin newtype so we can hang a generic
/// inherent [`DynStorage::transaction`] helper off the handle while keeping
/// the underlying [`Storage`] trait object-safe. All `Storage` accessors
/// (`storage.hosts()`, `storage.capabilities()`, ...) work via `Deref`.
#[derive(Clone)]
pub struct DynStorage(Arc<dyn Storage>);

impl std::ops::Deref for DynStorage {
    type Target = dyn Storage;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl DynStorage {
    pub fn new<S>(storage: S) -> Self
    where
        S: Storage + 'static,
    {
        Self(Arc::new(storage))
    }

    pub fn from_arc(storage: Arc<dyn Storage>) -> Self {
        Self(storage)
    }

    /// Clone the inner trait-object handle. Useful for decorator patterns
    /// (e.g. test fault injection) that want to wrap the live storage in a
    /// new `Storage` impl while sharing the same underlying state.
    pub fn arc(&self) -> Arc<dyn Storage> {
        Arc::clone(&self.0)
    }

    /// Run `work` inside a single backend transaction. The closure receives a
    /// [`TxStorage`] view over which sub-stores expose the same operations as
    /// the async [`Storage`] accessors, but synchronously and bound to the
    /// in-progress transaction. Cross-store mutations made inside the closure
    /// commit atomically; any error rolls everything back.
    ///
    /// Errors with [`AppError::unavailable`] if the backend doesn't support
    /// transactions. Check `storage.capabilities().strong_transactions` to
    /// gate calls when targeting unknown backends.
    pub async fn transaction<T, F>(&self, work: F) -> Result<T, AppError>
    where
        T: Send + 'static,
        F: FnOnce(&dyn TxStorage) -> Result<T, AppError> + Send + 'static,
    {
        let runner = self.transaction_runner().ok_or_else(|| {
            AppError::unavailable(
                "this storage backend does not support transactions; \
                 check capabilities().strong_transactions before calling",
            )
        })?;

        let erased: Box<dyn ErasedTxWork> = Box::new(tx::ClosureTxWork::new(work));
        let value = runner.run_transaction(erased).await?;
        value
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| AppError::internal("transaction result type mismatch"))
    }
}

// Type aliases for consumer convenience
pub type DynAttachmentStore = dyn AttachmentStore + Send + Sync;
pub type DynAttachmentCommunityAssignmentStore =
    dyn AttachmentCommunityAssignmentStore + Send + Sync;
pub type DynHostStore = dyn HostStore + Send + Sync;
pub type DynZoneStore = dyn ZoneStore + Send + Sync;
pub type DynRecordStore = dyn RecordStore + Send + Sync;
pub type DynLabelStore = dyn LabelStore + Send + Sync;
pub type DynNameServerStore = dyn NameServerStore + Send + Sync;
pub type DynNetworkStore = dyn NetworkStore + Send + Sync;
pub type DynHostContactStore = dyn HostContactStore + Send + Sync;
pub type DynHostGroupStore = dyn HostGroupStore + Send + Sync;
pub type DynBacnetStore = dyn BacnetStore + Send + Sync;
pub type DynPtrOverrideStore = dyn PtrOverrideStore + Send + Sync;
pub type DynNetworkPolicyStore = dyn NetworkPolicyStore + Send + Sync;
pub type DynCommunityStore = dyn CommunityStore + Send + Sync;
pub type DynHostCommunityAssignmentStore = dyn HostCommunityAssignmentStore + Send + Sync;
pub type DynTaskStore = dyn TaskStore + Send + Sync;
pub type DynImportStore = dyn ImportStore + Send + Sync;
pub type DynExportStore = dyn ExportStore + Send + Sync;
pub type DynAuditStore = dyn AuditStore + Send + Sync;
pub type DynAuthSessionStore = dyn AuthSessionStore + Send + Sync;
pub type DynHostPolicyStore = dyn HostPolicyStore + Send + Sync;
pub type DynHostViewStore = dyn HostViewStore + Send + Sync;

/// Identifies which storage backend is active at runtime.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackendKind {
    Memory,
    Postgres,
}

/// Describes the feature set of the active storage backend.
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct StorageCapabilities {
    pub persistent: bool,
    pub strong_transactions: bool,
    pub native_network_types: bool,
    pub skip_locked_task_claiming: bool,
    pub intended_for: Vec<String>,
}

/// Health check result from the storage backend.
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct StorageHealthReport {
    pub backend: StorageBackendKind,
    pub configured: bool,
    pub ready: bool,
    pub detail: String,
}

/// Composite storage interface providing access to all subsystem stores.
///
/// Each subsystem is accessed via its own trait (e.g., `hosts()` returns
/// a `HostStore`). Implementations handle cascading side-effects atomically.
#[async_trait]
pub trait Storage: Send + Sync {
    fn backend_kind(&self) -> StorageBackendKind;
    fn capabilities(&self) -> StorageCapabilities;
    async fn health(&self) -> Result<StorageHealthReport, AppError>;

    fn labels(&self) -> &(dyn LabelStore + Send + Sync);
    fn nameservers(&self) -> &(dyn NameServerStore + Send + Sync);
    fn zones(&self) -> &(dyn ZoneStore + Send + Sync);
    fn networks(&self) -> &(dyn NetworkStore + Send + Sync);
    fn hosts(&self) -> &(dyn HostStore + Send + Sync);
    fn attachments(&self) -> &(dyn AttachmentStore + Send + Sync);
    fn host_contacts(&self) -> &(dyn HostContactStore + Send + Sync);
    fn host_groups(&self) -> &(dyn HostGroupStore + Send + Sync);
    fn bacnet(&self) -> &(dyn BacnetStore + Send + Sync);
    fn ptr_overrides(&self) -> &(dyn PtrOverrideStore + Send + Sync);
    fn network_policies(&self) -> &(dyn NetworkPolicyStore + Send + Sync);
    fn communities(&self) -> &(dyn CommunityStore + Send + Sync);
    fn attachment_community_assignments(
        &self,
    ) -> &(dyn AttachmentCommunityAssignmentStore + Send + Sync);
    fn host_community_assignments(&self) -> &(dyn HostCommunityAssignmentStore + Send + Sync);
    fn tasks(&self) -> &(dyn TaskStore + Send + Sync);
    fn imports(&self) -> &(dyn ImportStore + Send + Sync);
    fn exports(&self) -> &(dyn ExportStore + Send + Sync);
    fn records(&self) -> &(dyn RecordStore + Send + Sync);
    fn audit(&self) -> &(dyn AuditStore + Send + Sync);
    fn auth_sessions(&self) -> &(dyn AuthSessionStore + Send + Sync);
    fn host_policy(&self) -> &(dyn HostPolicyStore + Send + Sync);
    fn host_views(&self) -> &(dyn HostViewStore + Send + Sync);

    /// Backends that support atomic multi-store mutations return their
    /// [`TransactionRunner`] here. Backends that don't return `None` (the
    /// default), and callers fall back to the per-store async API.
    ///
    /// Use [`DynStorage::transaction`] rather than calling this directly.
    fn transaction_runner(&self) -> Option<&(dyn TransactionRunner + Send + Sync)> {
        None
    }
}

/// Construct the storage backend based on configuration (auto, memory, or postgres).
pub fn build_storage(config: &Config) -> Result<DynStorage, AppError> {
    match config.storage_backend.resolve(config) {
        StorageBackendKind::Memory => Ok(DynStorage::new(memory::MemoryStorage::new())),
        StorageBackendKind::Postgres => {
            let database = Database::connect(config)?;
            if !database.is_configured() {
                return Err(AppError::config(
                    "postgres storage selected but MREG_DATABASE_URL is not configured",
                ));
            }
            Ok(DynStorage::new(postgres::PostgresStorage::new(database)))
        }
    }
}

impl StorageBackendSetting {
    pub fn resolve(&self, config: &Config) -> StorageBackendKind {
        match self {
            StorageBackendSetting::Auto => {
                if config.database_url.is_some() {
                    StorageBackendKind::Postgres
                } else {
                    StorageBackendKind::Memory
                }
            }
            StorageBackendSetting::Memory => StorageBackendKind::Memory,
            StorageBackendSetting::Postgres => StorageBackendKind::Postgres,
        }
    }
}
