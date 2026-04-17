// Backend modules
pub mod memory;
pub mod postgres;

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

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;

use crate::{
    config::{Config, StorageBackendSetting},
    db::Database,
    errors::AppError,
};

/// Thread-safe shared storage handle used across HTTP handlers.
pub type DynStorage = Arc<dyn Storage>;

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
}

/// Construct the storage backend based on configuration (auto, memory, or postgres).
pub fn build_storage(config: &Config) -> Result<DynStorage, AppError> {
    match config.storage_backend.resolve(config) {
        StorageBackendKind::Memory => Ok(Arc::new(memory::MemoryStorage::new())),
        StorageBackendKind::Postgres => {
            let database = Database::connect(config)?;
            if !database.is_configured() {
                return Err(AppError::config(
                    "postgres storage selected but MREG_DATABASE_URL is not configured",
                ));
            }
            Ok(Arc::new(postgres::PostgresStorage::new(database)))
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
