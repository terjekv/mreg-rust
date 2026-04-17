#![allow(dead_code)]

use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};

use actix_web::{App, body::to_bytes, http::StatusCode, test, web};
use serde_json::Value;
use uuid::Uuid;

use mreg_rust::{
    AppState, BuildInfo,
    authn::AuthnClient,
    authz::AuthorizerClient,
    config::{Config, StorageBackendSetting},
    events::EventSinkClient,
    storage::build_storage,
};

#[derive(Clone)]
pub struct TestCtx {
    state: AppState,
    namespace: String,
    id: u64,
}

impl TestCtx {
    pub fn treetop_memory() -> Option<Self> {
        Some(Self::new(treetop_memory_state()?))
    }

    fn new(state: AppState) -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            state,
            namespace: format!("t{:04x}{:x}", run_nonce(), id),
            id,
        }
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

    pub async fn get_status_as(&self, uri: &str, user: &str, groups: &[&str]) -> StatusCode {
        self.call_status(authenticated_request(
            test::TestRequest::get().uri(uri),
            user,
            groups,
        ))
        .await
    }

    pub async fn get_json_as(&self, uri: &str, user: &str, groups: &[&str]) -> (StatusCode, Value) {
        self.call_json(authenticated_request(
            test::TestRequest::get().uri(uri),
            user,
            groups,
        ))
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

    async fn call_status(&self, request: actix_http::Request) -> StatusCode {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(self.state.clone()))
                .wrap(mreg_rust::middleware::Authn)
                .configure(mreg_rust::api::v1::configure),
        )
        .await;
        test::call_service(&app, request).await.status()
    }

    async fn call_json(&self, request: actix_http::Request) -> (StatusCode, Value) {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(self.state.clone()))
                .wrap(mreg_rust::middleware::Authn)
                .configure(mreg_rust::api::v1::configure),
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

fn treetop_memory_state() -> Option<AppState> {
    let treetop_url = std::env::var("MREG_TEST_TREETOP_URL").ok()?;
    let config = Config {
        workers: Some(1),
        run_migrations: false,
        storage_backend: StorageBackendSetting::Memory,
        treetop_url: Some(treetop_url),
        treetop_timeout_ms: 1000,
        allow_dev_authz_bypass: false,
        ..Config::default()
    };
    let storage = build_storage(&config).ok()?;
    let authn = AuthnClient::from_config(&config, storage.clone()).ok()?;
    let authz = AuthorizerClient::from_config(&config);

    Some(AppState {
        config: Arc::new(config),
        build_info: BuildInfo::current(),
        storage,
        authn,
        authz,
        events: EventSinkClient::noop(),
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
