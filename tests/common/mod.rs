#![allow(dead_code)]

//! Shared test infrastructure for backend test suites.
//!
//! The dual-backend tests use namespaced data so memory and postgres can share
//! the same scenario bodies without database resets or ordering dependencies.

use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};

use actix_web::{App, body::to_bytes, http::StatusCode, test, web};
use serde_json::Value;
use tokio::sync::OnceCell;
use uuid::Uuid;

use mreg_rust::{
    AppState, BuildInfo,
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::{Config, StorageBackendSetting},
    db::{QueryCaptureSnapshot, take_query_capture, with_query_capture},
    events::EventSinkClient,
    services::Services,
    storage::{DynStorage, ReadableStorage, build_storage},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestBackend {
    Memory,
    Postgres,
}

impl TestBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Postgres => "postgres",
        }
    }
}

#[derive(Clone)]
pub struct TestCtx {
    backend: TestBackend,
    state: AppState,
    namespace: String,
    id: u64,
}

impl TestCtx {
    pub fn memory() -> Self {
        Self::new(TestBackend::Memory, memory_state())
    }

    pub fn memory_with_auto_dhcp() -> Self {
        Self::new(TestBackend::Memory, memory_state_with_auto_dhcp())
    }

    pub fn treetop_memory() -> Option<Self> {
        Some(Self::new(TestBackend::Memory, treetop_memory_state()?))
    }

    pub async fn postgres() -> Option<Self> {
        Some(Self::new(TestBackend::Postgres, postgres_state().await?))
    }

    pub async fn postgres_with_auto_dhcp() -> Option<Self> {
        Some(Self::new(
            TestBackend::Postgres,
            postgres_state_with_auto_dhcp().await?,
        ))
    }

    fn new(backend: TestBackend, state: AppState) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            backend,
            state,
            namespace: format!("t{:04x}{:x}", run_nonce(), id),
            id,
        }
    }

    pub fn backend(&self) -> TestBackend {
        self.backend
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn storage(&self) -> DynStorage {
        self.state.services.inner_storage()
    }

    pub fn name(&self, stem: &str) -> String {
        format!("{}-{}", sanitize(stem), self.namespace)
    }

    pub fn zone(&self, stem: &str) -> String {
        format!("{}.test", self.name(stem))
    }

    pub fn host(&self, stem: &str) -> String {
        format!("{}.test", self.name(stem))
    }

    pub fn host_in_zone(&self, stem: &str, zone: &str) -> String {
        format!("{}.{}", self.name(stem), zone)
    }

    pub fn nameserver(&self, stem: &str, zone: &str) -> String {
        format!("{}.{}", self.name(stem), zone)
    }

    pub fn cidr(&self, slot: u16) -> String {
        let subnet = run_subnet_offset()
            .wrapping_add((self.id as u16).wrapping_mul(16))
            .wrapping_add(slot);
        let octet_2 = ((subnet >> 8) & 0xff) as u8;
        let octet_3 = (subnet & 0xff) as u8;
        format!("10.{octet_2}.{octet_3}.0/24")
    }

    pub fn ip_in_cidr(&self, cidr: &str, host_octet: u8) -> String {
        let prefix = cidr.strip_suffix("/24").expect("expected /24 cidr");
        let mut parts = prefix
            .split('.')
            .map(str::to_string)
            .collect::<Vec<String>>();
        parts[3] = host_octet.to_string();
        parts.join(".")
    }

    pub fn bacnet_id(&self, slot: u32) -> u32 {
        (((run_nonce() as u32) << 16) | ((self.id as u32) << 8)).saturating_add(slot.max(1))
    }

    pub async fn get_status(&self, uri: &str) -> StatusCode {
        self.call_status(test::TestRequest::get().uri(uri).to_request())
            .await
    }

    pub async fn get_status_as(&self, uri: &str, user: &str, groups: &[&str]) -> StatusCode {
        self.call_status(authenticated_request(
            test::TestRequest::get().uri(uri),
            user,
            groups,
        ))
        .await
    }

    pub async fn get_json(&self, uri: &str) -> Value {
        let (status, body) = self
            .call_json(test::TestRequest::get().uri(uri).to_request())
            .await;
        assert_eq!(status, StatusCode::OK, "GET {uri} failed with {status}");
        body
    }

    pub async fn get_json_as(&self, uri: &str, user: &str, groups: &[&str]) -> (StatusCode, Value) {
        self.call_json(authenticated_request(
            test::TestRequest::get().uri(uri),
            user,
            groups,
        ))
        .await
    }

    pub async fn get_json_with_query_capture(
        &self,
        uri: &str,
        label: &str,
    ) -> (Value, QueryCaptureSnapshot) {
        let capture_id = format!("{}-{label}", self.namespace);
        let (status, body) = with_query_capture(
            capture_id.clone(),
            self.call_json(test::TestRequest::get().uri(uri).to_request()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "GET {uri} failed with {status}");
        (body, take_query_capture(&capture_id))
    }

    pub async fn post(&self, uri: &str, body: Value) -> StatusCode {
        self.call_status(
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await
    }

    pub async fn post_as(&self, uri: &str, body: Value, user: &str, groups: &[&str]) -> StatusCode {
        self.call_status(authenticated_request(
            test::TestRequest::post().uri(uri).set_json(body),
            user,
            groups,
        ))
        .await
    }

    pub async fn post_json(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call_json(
            test::TestRequest::post()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await
    }

    pub async fn post_json_as(
        &self,
        uri: &str,
        body: Value,
        user: &str,
        groups: &[&str],
    ) -> (StatusCode, Value) {
        self.call_json(authenticated_request(
            test::TestRequest::post().uri(uri).set_json(body),
            user,
            groups,
        ))
        .await
    }

    pub async fn patch(&self, uri: &str, body: Value) -> StatusCode {
        self.call_status(
            test::TestRequest::patch()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await
    }

    pub async fn patch_as(
        &self,
        uri: &str,
        body: Value,
        user: &str,
        groups: &[&str],
    ) -> StatusCode {
        self.call_status(authenticated_request(
            test::TestRequest::patch().uri(uri).set_json(body),
            user,
            groups,
        ))
        .await
    }

    pub async fn patch_json(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.call_json(
            test::TestRequest::patch()
                .uri(uri)
                .set_json(body)
                .to_request(),
        )
        .await
    }

    pub async fn delete(&self, uri: &str) -> StatusCode {
        self.call_status(test::TestRequest::delete().uri(uri).to_request())
            .await
    }

    pub async fn delete_as(&self, uri: &str, user: &str, groups: &[&str]) -> StatusCode {
        self.call_status(authenticated_request(
            test::TestRequest::delete().uri(uri),
            user,
            groups,
        ))
        .await
    }

    pub async fn seed_zone(&self, zone: &str, nameserver: &str) {
        let status = self
            .post(
                "/dns/nameservers",
                serde_json::json!({ "name": nameserver }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);

        let status = self
            .post(
                "/dns/forward-zones",
                serde_json::json!({
                    "name": zone,
                    "primary_ns": nameserver,
                    "nameservers": [nameserver],
                    "email": format!("hostmaster@{zone}"),
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    pub async fn seed_host(&self, host: &str) {
        let status = self
            .post(
                "/inventory/hosts",
                serde_json::json!({ "name": host, "comment": "test host" }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    pub async fn seed_host_in_zone(&self, host: &str, zone: &str) {
        let status = self
            .post(
                "/inventory/hosts",
                serde_json::json!({
                    "name": host,
                    "zone": zone,
                    "comment": "test host",
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    pub async fn seed_network(&self, cidr: &str) {
        let status = self
            .post(
                "/inventory/networks",
                serde_json::json!({
                    "cidr": cidr,
                    "description": format!("network {cidr}"),
                }),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    /// Assert that an audit event with the given resource_kind, resource_name, and action
    /// exists in the history. Does NOT assume ordering — safe for parallel tests.
    pub async fn assert_audit_exists(
        &self,
        resource_kind: &str,
        resource_name: &str,
        action: &str,
    ) {
        let body = self.get_json("/system/history").await;
        let events = body["items"].as_array().expect("history items");
        let found = events.iter().any(|e| {
            e["resource_kind"] == resource_kind
                && e["resource_name"] == resource_name
                && e["action"] == action
        });
        assert!(
            found,
            "expected audit event: {resource_kind}/{resource_name}/{action} not found in {} events",
            events.len()
        );
    }

    /// Find an audit event and return it for further assertions.
    pub async fn find_audit_event(
        &self,
        resource_kind: &str,
        resource_name: &str,
        action: &str,
    ) -> Value {
        let body = self.get_json("/system/history").await;
        let events = body["items"].as_array().expect("history items");
        events
            .iter()
            .find(|e| {
                e["resource_kind"] == resource_kind
                    && e["resource_name"] == resource_name
                    && e["action"] == action
            })
            .cloned()
            .unwrap_or_else(|| {
                panic!("audit event not found: {resource_kind}/{resource_name}/{action}")
            })
    }

    async fn call_status(&self, request: actix_http::Request) -> StatusCode {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(self.state.clone()))
                .wrap(mreg_rust::middleware::Authn)
                .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
        )
        .await;
        test::call_service(&app, request).await.status()
    }

    async fn call_json(&self, request: actix_http::Request) -> (StatusCode, Value) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(self.state.clone()))
                .wrap(mreg_rust::middleware::Authn)
                .configure(|cfg| mreg_rust::api::v1::configure(cfg, false)),
        )
        .await;
        let response = test::call_service(&app, request).await;
        let status = response.status();
        let bytes = to_bytes(response.into_body())
            .await
            .expect("read response body");
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("response body should be valid json")
        };
        (status, body)
    }
}

/// Create a fresh memory-backed AppState.
pub fn memory_state() -> AppState {
    build_state(
        StorageBackendSetting::Memory,
        None,
        false,
        None,
        true,
        false,
        false,
    )
    .expect("memory storage")
}

/// Create a fresh memory-backed AppState with auto-DHCP flags enabled.
pub fn memory_state_with_auto_dhcp() -> AppState {
    build_state(
        StorageBackendSetting::Memory,
        None,
        false,
        None,
        true,
        true,
        true,
    )
    .expect("memory storage")
}

/// Create a fresh memory-backed AppState that delegates authz to a live treetop server.
///
/// Returns `None` when `MREG_TEST_TREETOP_URL` is not configured.
pub fn treetop_memory_state() -> Option<AppState> {
    let treetop_url = std::env::var("MREG_TEST_TREETOP_URL").ok()?;
    build_state(
        StorageBackendSetting::Memory,
        None,
        false,
        Some(treetop_url),
        false,
        false,
        false,
    )
    .ok()
}

/// Shared postgres AppState initialized once. Returns None when the test
/// database URL is not configured. Panics in CI if the URL is set but
/// the database is unreachable.
pub async fn postgres_state() -> Option<AppState> {
    static PG_STATE: OnceCell<Option<AppState>> = OnceCell::const_new();

    PG_STATE
        .get_or_init(|| async {
            let url = std::env::var("MREG_TEST_DATABASE_URL").ok()?;
            let result = build_state(
                StorageBackendSetting::Postgres,
                Some(url),
                true,
                None,
                true,
                false,
                false,
            );
            match result {
                Ok(state) => Some(state),
                Err(error) => {
                    if std::env::var("CI").is_ok() {
                        panic!(
                            "FATAL in CI: MREG_TEST_DATABASE_URL is set but database is \
                             unreachable: {error}"
                        );
                    }
                    eprintln!(
                        "warning: MREG_TEST_DATABASE_URL is set but database is \
                         unreachable: {error}. Postgres tests will be skipped."
                    );
                    None
                }
            }
        })
        .await
        .clone()
}

/// Shared postgres AppState with auto-DHCP flags enabled. Returns None when
/// the test database URL is not configured. Panics in CI if unreachable.
pub async fn postgres_state_with_auto_dhcp() -> Option<AppState> {
    static PG_STATE_DHCP: OnceCell<Option<AppState>> = OnceCell::const_new();

    PG_STATE_DHCP
        .get_or_init(|| async {
            let url = std::env::var("MREG_TEST_DATABASE_URL").ok()?;
            let result = build_state(
                StorageBackendSetting::Postgres,
                Some(url),
                true,
                None,
                true,
                true,
                true,
            );
            match result {
                Ok(state) => Some(state),
                Err(error) => {
                    if std::env::var("CI").is_ok() {
                        panic!(
                            "FATAL in CI: MREG_TEST_DATABASE_URL is set but database is \
                             unreachable: {error}"
                        );
                    }
                    eprintln!(
                        "warning: MREG_TEST_DATABASE_URL is set but database is \
                         unreachable (auto-dhcp): {error}. Postgres tests will be skipped."
                    );
                    None
                }
            }
        })
        .await
        .clone()
}

pub fn postgres_skip_message(scope: &str) -> String {
    let message = format!(
        "skipping {scope}: PostgreSQL-backed tests require MREG_TEST_DATABASE_URL. \
Set it to a disposable test database, for example \
MREG_TEST_DATABASE_URL=postgres://mreg:mreg@localhost:5433/mreg_test. \
README.md documents a local Docker PostgreSQL setup, and CI runs the same tests \
against a Postgres service container."
    );
    if std::env::var("CI").is_ok() {
        panic!(
            "FATAL in CI: {scope}: MREG_TEST_DATABASE_URL is not set. \
Postgres tests must not be silently skipped in CI."
        );
    }
    message
}

fn build_state(
    backend: StorageBackendSetting,
    database_url: Option<String>,
    run_migrations: bool,
    treetop_url: Option<String>,
    allow_dev_authz_bypass: bool,
    dhcp_auto_v4_client_id: bool,
    dhcp_auto_v6_duid_ll: bool,
) -> Result<AppState, mreg_rust::errors::AppError> {
    let config = Config {
        workers: Some(1),
        database_url,
        run_migrations,
        storage_backend: backend,
        treetop_url,
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass,
        dhcp_auto_v4_client_id,
        dhcp_auto_v6_duid_ll,
        ..Config::default()
    };
    let storage = build_storage(&config)?;
    let authn = AuthnClient::from_config(&config, storage.clone())?;
    let authz = AuthorizerClient::from_config(&config).expect("authz config");
    let events = EventSinkClient::noop();
    let reader = ReadableStorage::new(storage.clone());
    let services = Services::new(storage, events);

    Ok(AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        reader,
        services,
        authn,
        authz,
    })
}

fn sanitize(stem: &str) -> String {
    stem.chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '-',
        })
        .collect()
}

fn run_nonce() -> u16 {
    static RUN_NONCE: OnceLock<u16> = OnceLock::new();
    *RUN_NONCE.get_or_init(|| {
        let bytes = Uuid::new_v4().into_bytes();
        u16::from_be_bytes([bytes[0], bytes[1]])
    })
}

fn run_subnet_offset() -> u16 {
    static SUBNET_OFFSET: OnceLock<u16> = OnceLock::new();
    *SUBNET_OFFSET.get_or_init(|| {
        let bytes = Uuid::new_v4().into_bytes();
        u16::from_be_bytes([bytes[2], bytes[3]])
    })
}

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn authenticated_request(
    request: test::TestRequest,
    user: &str,
    groups: &[&str],
) -> actix_http::Request {
    let request = request.insert_header(("X-Mreg-User", user));
    let request = if groups.is_empty() {
        request
    } else {
        request.insert_header(("X-Mreg-Groups", groups.join(",")))
    };
    request.to_request()
}

/// Thin wrapper that generates `$name::memory` and `$name::postgres` variants
/// from a `|ctx| { ... }` block.
#[macro_export]
macro_rules! dual_backend_test {
    ($test_name:ident, |$ctx:ident| $body:block) => {
        mod $test_name {
            use super::*;

            #[actix_web::test]
            async fn memory() {
                let $ctx = common::TestCtx::memory();
                $body
            }

            #[actix_web::test]
            async fn postgres() {
                let Some($ctx) = common::TestCtx::postgres().await else {
                    eprintln!("{}", common::postgres_skip_message(stringify!($test_name)));
                    return;
                };
                $body
            }
        }
    };
}

/// Like `dual_backend_test!` but creates contexts with auto-DHCP flags enabled.
#[macro_export]
macro_rules! dual_backend_test_auto_dhcp {
    ($test_name:ident, |$ctx:ident| $body:block) => {
        mod $test_name {
            use super::*;

            #[actix_web::test]
            async fn memory() {
                let $ctx = common::TestCtx::memory_with_auto_dhcp();
                $body
            }

            #[actix_web::test]
            async fn postgres() {
                let Some($ctx) = common::TestCtx::postgres_with_auto_dhcp().await else {
                    eprintln!("{}", common::postgres_skip_message(stringify!($test_name)));
                    return;
                };
                $body
            }
        }
    };
}
