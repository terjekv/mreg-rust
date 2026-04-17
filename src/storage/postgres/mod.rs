mod attachments;
mod audit;
mod auth_sessions;
mod bacnet_ids;
mod communities;
mod exports;
pub mod helpers;
mod host_community_assignments;
mod host_contacts;
mod host_groups;
mod host_policy;
mod hosts;
mod imports;
mod labels;
mod nameservers;
mod network_policies;
mod networks;
mod ptr_overrides;
mod records;
mod tasks;
mod zones;

use async_trait::async_trait;
use diesel::{
    ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl, sql_query,
};
use uuid::Uuid;

use crate::{
    db::Database,
    errors::AppError,
    storage::{
        AttachmentCommunityAssignmentStore, AttachmentStore, AuditStore, AuthSessionStore,
        BacnetStore, CommunityStore, ExportStore, HostCommunityAssignmentStore, HostContactStore,
        HostGroupStore, HostPolicyStore, HostStore, ImportStore, LabelStore, NameServerStore,
        NetworkPolicyStore, NetworkStore, PtrOverrideStore, RecordStore, Storage,
        StorageBackendKind, StorageCapabilities, StorageHealthReport, TaskStore, ZoneStore,
    },
};

#[derive(Clone)]
pub struct PostgresStorage {
    pub(crate) database: Database,
}

impl PostgresStorage {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl Storage for PostgresStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        StorageBackendKind::Postgres
    }

    fn capabilities(&self) -> StorageCapabilities {
        StorageCapabilities {
            persistent: true,
            strong_transactions: true,
            native_network_types: true,
            skip_locked_task_claiming: true,
            intended_for: vec![
                "production".to_string(),
                "integration_tests".to_string(),
                "realistic_local_dev".to_string(),
            ],
        }
    }

    async fn health(&self) -> Result<StorageHealthReport, AppError> {
        let Some(pool) = self.database.pool() else {
            return Ok(StorageHealthReport {
                backend: StorageBackendKind::Postgres,
                configured: false,
                ready: false,
                detail: "postgres storage has no configured pool".to_string(),
            });
        };

        let mut connection = pool.get().map_err(AppError::internal)?;
        sql_query("SELECT 1").execute(&mut connection)?;
        Self::ensure_builtin_record_types(&mut connection)?;

        Ok(StorageHealthReport {
            backend: StorageBackendKind::Postgres,
            configured: true,
            ready: true,
            detail: "postgres storage connection succeeded".to_string(),
        })
    }

    fn labels(&self) -> &(dyn LabelStore + Send + Sync) {
        self
    }

    fn nameservers(&self) -> &(dyn NameServerStore + Send + Sync) {
        self
    }

    fn zones(&self) -> &(dyn ZoneStore + Send + Sync) {
        self
    }

    fn networks(&self) -> &(dyn NetworkStore + Send + Sync) {
        self
    }

    fn hosts(&self) -> &(dyn HostStore + Send + Sync) {
        self
    }

    fn attachments(&self) -> &(dyn AttachmentStore + Send + Sync) {
        self
    }

    fn host_contacts(&self) -> &(dyn HostContactStore + Send + Sync) {
        self
    }

    fn host_groups(&self) -> &(dyn HostGroupStore + Send + Sync) {
        self
    }

    fn bacnet(&self) -> &(dyn BacnetStore + Send + Sync) {
        self
    }

    fn ptr_overrides(&self) -> &(dyn PtrOverrideStore + Send + Sync) {
        self
    }

    fn network_policies(&self) -> &(dyn NetworkPolicyStore + Send + Sync) {
        self
    }

    fn communities(&self) -> &(dyn CommunityStore + Send + Sync) {
        self
    }

    fn attachment_community_assignments(
        &self,
    ) -> &(dyn AttachmentCommunityAssignmentStore + Send + Sync) {
        self
    }

    fn host_community_assignments(&self) -> &(dyn HostCommunityAssignmentStore + Send + Sync) {
        self
    }

    fn tasks(&self) -> &(dyn TaskStore + Send + Sync) {
        self
    }

    fn imports(&self) -> &(dyn ImportStore + Send + Sync) {
        self
    }

    fn exports(&self) -> &(dyn ExportStore + Send + Sync) {
        self
    }

    fn records(&self) -> &(dyn RecordStore + Send + Sync) {
        self
    }

    fn audit(&self) -> &(dyn AuditStore + Send + Sync) {
        self
    }

    fn auth_sessions(&self) -> &(dyn AuthSessionStore + Send + Sync) {
        self
    }

    fn host_policy(&self) -> &(dyn HostPolicyStore + Send + Sync) {
        self
    }
}

// ---------------------------------------------------------------------------
// Shared resolution helpers used by multiple sub-modules
// ---------------------------------------------------------------------------

impl PostgresStorage {
    pub(in crate::storage::postgres) fn resolve_host_id(
        connection: &mut PgConnection,
        name: &crate::domain::types::Hostname,
    ) -> Result<Uuid, AppError> {
        use crate::db::schema::hosts;

        hosts::table
            .filter(hosts::name.eq(name.as_str()))
            .select(hosts::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found(format!("host '{}' was not found", name.as_str())))
    }

    pub(in crate::storage::postgres) fn resolve_host_group_id(
        connection: &mut PgConnection,
        name: &crate::domain::types::HostGroupName,
    ) -> Result<Uuid, AppError> {
        use crate::db::schema::host_groups;

        host_groups::table
            .filter(host_groups::name.eq(name.as_str()))
            .select(host_groups::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!("host group '{}' was not found", name.as_str()))
            })
    }

    pub(in crate::storage::postgres) fn resolve_network_policy_id(
        connection: &mut PgConnection,
        name: &crate::domain::types::NetworkPolicyName,
    ) -> Result<Uuid, AppError> {
        use crate::db::schema::network_policies;

        network_policies::table
            .filter(network_policies::name.eq(name.as_str()))
            .select(network_policies::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!("network policy '{}' was not found", name.as_str()))
            })
    }
}
