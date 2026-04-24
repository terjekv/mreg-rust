//! Cross-store transaction atomicity test.
//!
//! Wraps the live `DynStorage` in a `FailingAuditStorage` decorator that lets
//! every accessor through to the inner backend but injects an error from
//! `TxAuditStore::record_event` inside `TransactionRunner::run_transaction`.
//! `services::hosts::delete` runs the host delete and the audit insert in the
//! same transaction, so the injected audit failure must roll the host delete
//! back. This exercises both the Memory snapshot/swap path and the Postgres
//! Diesel `connection.transaction(...)` path.

mod common;

use std::sync::Arc;

use async_trait::async_trait;
use common::TestCtx;

use mreg_rust::{
    audit::{CreateHistoryEvent, HistoryEvent},
    domain::{
        pagination::{Page, PageRequest},
        types::Hostname,
    },
    errors::AppError,
    events::EventSinkClient,
    services,
    storage::{
        AttachmentCommunityAssignmentStore, AttachmentStore, AuditStore, BacnetStore,
        CommunityStore, DynStorage, ErasedTxWork, ExportStore, HostCommunityAssignmentStore,
        HostContactStore, HostGroupStore, HostPolicyStore, HostStore, HostViewStore, ImportStore,
        LabelStore, NameServerStore, NetworkPolicyStore, NetworkStore, PtrOverrideStore,
        RecordStore, Storage, StorageBackendKind, StorageCapabilities, StorageHealthReport,
        TaskStore, TransactionRunner, TxAttachmentCommunityAssignmentStore, TxAttachmentStore,
        TxAuditStore, TxBacnetStore, TxCommunityStore, TxHostCommunityAssignmentStore,
        TxHostContactStore, TxHostGroupStore, TxHostPolicyStore, TxHostStore, TxLabelStore,
        TxNameServerStore, TxNetworkPolicyStore, TxNetworkStore, TxPtrOverrideStore,
        TxRecordStore, TxStorage, TxZoneStore, ZoneStore,
    },
};

/// Decorator that delegates every `Storage` accessor to the inner backend but
/// substitutes a failing `TxAuditStore` inside any transaction.
struct FailingAuditStorage {
    inner: Arc<dyn Storage>,
}

#[async_trait]
impl Storage for FailingAuditStorage {
    fn backend_kind(&self) -> StorageBackendKind {
        self.inner.backend_kind()
    }
    fn capabilities(&self) -> StorageCapabilities {
        self.inner.capabilities()
    }
    async fn health(&self) -> Result<StorageHealthReport, AppError> {
        self.inner.health().await
    }
    fn labels(&self) -> &(dyn LabelStore + Send + Sync) {
        self.inner.labels()
    }
    fn nameservers(&self) -> &(dyn NameServerStore + Send + Sync) {
        self.inner.nameservers()
    }
    fn zones(&self) -> &(dyn ZoneStore + Send + Sync) {
        self.inner.zones()
    }
    fn networks(&self) -> &(dyn NetworkStore + Send + Sync) {
        self.inner.networks()
    }
    fn hosts(&self) -> &(dyn HostStore + Send + Sync) {
        self.inner.hosts()
    }
    fn attachments(&self) -> &(dyn AttachmentStore + Send + Sync) {
        self.inner.attachments()
    }
    fn host_contacts(&self) -> &(dyn HostContactStore + Send + Sync) {
        self.inner.host_contacts()
    }
    fn host_groups(&self) -> &(dyn HostGroupStore + Send + Sync) {
        self.inner.host_groups()
    }
    fn bacnet(&self) -> &(dyn BacnetStore + Send + Sync) {
        self.inner.bacnet()
    }
    fn ptr_overrides(&self) -> &(dyn PtrOverrideStore + Send + Sync) {
        self.inner.ptr_overrides()
    }
    fn network_policies(&self) -> &(dyn NetworkPolicyStore + Send + Sync) {
        self.inner.network_policies()
    }
    fn communities(&self) -> &(dyn CommunityStore + Send + Sync) {
        self.inner.communities()
    }
    fn attachment_community_assignments(
        &self,
    ) -> &(dyn AttachmentCommunityAssignmentStore + Send + Sync) {
        self.inner.attachment_community_assignments()
    }
    fn host_community_assignments(&self) -> &(dyn HostCommunityAssignmentStore + Send + Sync) {
        self.inner.host_community_assignments()
    }
    fn tasks(&self) -> &(dyn TaskStore + Send + Sync) {
        self.inner.tasks()
    }
    fn imports(&self) -> &(dyn ImportStore + Send + Sync) {
        self.inner.imports()
    }
    fn exports(&self) -> &(dyn ExportStore + Send + Sync) {
        self.inner.exports()
    }
    fn records(&self) -> &(dyn RecordStore + Send + Sync) {
        self.inner.records()
    }
    fn audit(&self) -> &(dyn AuditStore + Send + Sync) {
        self.inner.audit()
    }
    fn auth_sessions(&self) -> &(dyn mreg_rust::storage::AuthSessionStore + Send + Sync) {
        self.inner.auth_sessions()
    }
    fn host_policy(&self) -> &(dyn HostPolicyStore + Send + Sync) {
        self.inner.host_policy()
    }
    fn host_views(&self) -> &(dyn HostViewStore + Send + Sync) {
        self.inner.host_views()
    }
    fn transaction_runner(&self) -> Option<&(dyn TransactionRunner + Send + Sync)> {
        Some(self)
    }
}

#[async_trait]
impl TransactionRunner for FailingAuditStorage {
    async fn run_transaction(
        &self,
        work: Box<dyn ErasedTxWork>,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError> {
        let inner_runner = self
            .inner
            .transaction_runner()
            .expect("inner storage must support transactions for fault-injection test");
        let wrapped: Box<dyn ErasedTxWork> = Box::new(WrappedWork { inner: work });
        inner_runner.run_transaction(wrapped).await
    }
}

/// Adapter that re-runs the original work but with a `FailingAuditTxStorage`
/// substituted for the backend's real `TxStorage`.
struct WrappedWork {
    inner: Box<dyn ErasedTxWork>,
}

impl ErasedTxWork for WrappedWork {
    fn run(
        self: Box<Self>,
        tx: &dyn TxStorage,
    ) -> Result<Box<dyn std::any::Any + Send>, AppError> {
        let failing = FailingAuditTxStorage { inner: tx };
        self.inner.run(&failing)
    }
}

struct FailingAuditTxStorage<'a> {
    inner: &'a dyn TxStorage,
}

impl<'a> TxStorage for FailingAuditTxStorage<'a> {
    fn labels(&self) -> &dyn TxLabelStore {
        self.inner.labels()
    }
    fn nameservers(&self) -> &dyn TxNameServerStore {
        self.inner.nameservers()
    }
    fn zones(&self) -> &dyn TxZoneStore {
        self.inner.zones()
    }
    fn networks(&self) -> &dyn TxNetworkStore {
        self.inner.networks()
    }
    fn hosts(&self) -> &dyn TxHostStore {
        self.inner.hosts()
    }
    fn attachments(&self) -> &dyn TxAttachmentStore {
        self.inner.attachments()
    }
    fn attachment_community_assignments(&self) -> &dyn TxAttachmentCommunityAssignmentStore {
        self.inner.attachment_community_assignments()
    }
    fn host_contacts(&self) -> &dyn TxHostContactStore {
        self.inner.host_contacts()
    }
    fn host_groups(&self) -> &dyn TxHostGroupStore {
        self.inner.host_groups()
    }
    fn bacnet(&self) -> &dyn TxBacnetStore {
        self.inner.bacnet()
    }
    fn ptr_overrides(&self) -> &dyn TxPtrOverrideStore {
        self.inner.ptr_overrides()
    }
    fn network_policies(&self) -> &dyn TxNetworkPolicyStore {
        self.inner.network_policies()
    }
    fn communities(&self) -> &dyn TxCommunityStore {
        self.inner.communities()
    }
    fn host_community_assignments(&self) -> &dyn TxHostCommunityAssignmentStore {
        self.inner.host_community_assignments()
    }
    fn host_policy(&self) -> &dyn TxHostPolicyStore {
        self.inner.host_policy()
    }
    fn records(&self) -> &dyn TxRecordStore {
        self.inner.records()
    }
    fn audit(&self) -> &dyn TxAuditStore {
        self
    }
}

impl<'a> TxAuditStore for FailingAuditTxStorage<'a> {
    fn record_event(&self, _event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        Err(AppError::internal("injected audit failure"))
    }
    fn list_events(&self, _page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        Err(AppError::internal("injected audit failure"))
    }
}

async fn run_scenario(ctx: TestCtx) {
    let host_name = ctx.host("rollback");
    ctx.seed_host(&host_name).await;

    let inner_arc = ctx.storage().arc();
    let failing = Arc::new(FailingAuditStorage {
        inner: inner_arc.clone(),
    });
    let wrapped = DynStorage::from_arc(failing);

    let events = EventSinkClient::noop();
    let name = Hostname::new(&host_name).expect("valid hostname");

    let result = services::hosts::delete(&wrapped, &name, &events).await;
    assert!(
        result.is_err(),
        "expected delete to fail because audit insert was injected to fail, got {:?}",
        result.as_ref().map(|_| "Ok"),
    );

    // The host must still exist on the inner backend: the audit failure must
    // have rolled the cascading host delete back.
    let still = ctx
        .storage()
        .hosts()
        .get_host_by_name(&name)
        .await
        .expect("host should still be present after rolled-back delete");
    assert_eq!(still.name().as_str(), name.as_str());
}

mod audit_failure_rolls_back_host_delete {
    use super::*;

    #[actix_web::test]
    async fn memory() {
        let ctx = TestCtx::memory();
        run_scenario(ctx).await;
    }

    #[actix_web::test]
    async fn postgres() {
        let Some(ctx) = TestCtx::postgres().await else {
            eprintln!(
                "{}",
                common::postgres_skip_message("audit_failure_rolls_back_host_delete")
            );
            return;
        };
        run_scenario(ctx).await;
    }
}
