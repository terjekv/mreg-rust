pub mod attachment_community_assignments;
pub mod attachments;
pub mod auth;
mod authz;
pub mod bacnet_ids;
pub mod communities;
pub mod dns;
pub mod host_community_assignments;
pub mod host_contacts;
pub mod host_groups;
pub mod host_policy;
pub mod hosts;
pub mod labels;
pub mod nameservers;
pub mod network_policies;
pub mod networks;
pub mod ptr_overrides;
pub mod records;
pub mod system;
pub mod workflows;
pub mod zones;

use actix_web::web;
use serde::Serialize;
use utoipa::ToSchema;

use crate::storage::StorageBackendKind;

/// Generic list response with a backend indicator, for system/diagnostic endpoints.
///
/// Use `SystemListResponse::from_page` to build from a [`Page`].
#[derive(Serialize, ToSchema)]
pub struct SystemListResponse {
    /// Items in this page, serialized as JSON values.
    #[schema(value_type = Vec<Object>)]
    pub items: Vec<serde_json::Value>,
    pub total: u64,
    pub next_cursor: Option<uuid::Uuid>,
    pub backend: StorageBackendKind,
}

impl SystemListResponse {
    pub(crate) fn from_page<T: Serialize>(
        page: crate::domain::pagination::Page<T>,
        backend: StorageBackendKind,
    ) -> Self {
        Self {
            items: page
                .items
                .iter()
                .map(|item| serde_json::to_value(item).unwrap_or_default())
                .collect(),
            total: page.total,
            next_cursor: page.next_cursor,
            backend,
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(system::configure)
        .configure(dns::configure)
        .configure(auth::configure)
        .configure(attachment_community_assignments::configure)
        .configure(attachments::configure)
        .configure(bacnet_ids::configure)
        .configure(communities::configure)
        .configure(host_community_assignments::configure)
        .configure(host_contacts::configure)
        .configure(host_groups::configure)
        .configure(network_policies::configure)
        .configure(networks::configure)
        .configure(host_policy::configure)
        .configure(hosts::configure)
        .configure(labels::configure)
        .configure(nameservers::configure)
        .configure(ptr_overrides::configure)
        .configure(records::configure)
        .configure(workflows::configure)
        .configure(zones::configure);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use actix_web::{App, http::StatusCode, test, web};

    use crate::{
        AppState, BuildInfo,
        authn::AuthnClient,
        authz::AuthorizerClient,
        config::{Config, StorageBackendSetting},
        events::EventSinkClient,
        services::Services,
        storage::{ReadableStorage, build_storage},
    };

    pub(crate) fn test_state() -> AppState {
        test_state_with_payload_limit(1024 * 1024)
    }

    fn test_state_with_payload_limit(json_payload_limit_bytes: usize) -> AppState {
        let config = Config {
            workers: Some(1),
            json_payload_limit_bytes,
            run_migrations: false,
            storage_backend: StorageBackendSetting::Memory,
            treetop_timeout_ms: 1000,
            allow_dev_authz_bypass: true,
            ..Config::default()
        };

        let storage = build_storage(&config).expect("memory storage should initialize");
        let authn = AuthnClient::from_config(&config, storage.clone()).expect("authn config");
        let authz = AuthorizerClient::from_config(&config).expect("authz config");
        let events = EventSinkClient::noop();
        let reader = ReadableStorage::new(storage.clone());
        let services = Services::new(storage, events);

        AppState {
            config: Arc::new(config),
            build_info: BuildInfo::current(),
            reader,
            services,
            authn,
            authz,
        }
    }

    #[actix_web::test]
    async fn health_endpoint_reports_storage_backend() {
        let state = test_state();
        let json_limit = state.config.json_payload_limit_bytes;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .app_data(crate::api::json_config(json_limit))
                .configure(super::configure),
        )
        .await;

        let request = test::TestRequest::get().uri("/system/health").to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["storage"]["backend"], "memory");
        assert_eq!(body["storage"]["ready"], true);
        assert_eq!(body["authz"], "bypass");
    }

    #[actix_web::test]
    async fn version_endpoint_reports_api_base() {
        let state = test_state();
        let json_limit = state.config.json_payload_limit_bytes;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .app_data(crate::api::json_config(json_limit))
                .configure(super::configure),
        )
        .await;

        let request = test::TestRequest::get().uri("/system/version").to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["api_base"], "/api/v1");
        assert_eq!(body["service"], env!("CARGO_PKG_NAME"));
    }

    #[actix_web::test]
    async fn tasks_endpoint_uses_storage_backend() {
        let state = test_state();
        let json_limit = state.config.json_payload_limit_bytes;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .app_data(crate::api::json_config(json_limit))
                .configure(super::configure),
        )
        .await;

        let request = test::TestRequest::get()
            .uri("/workflows/tasks")
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["backend"], "memory");
        assert_eq!(body["items"], serde_json::json!([]));
    }

    #[actix_web::test]
    async fn json_payload_limit_rejects_oversized_bodies() {
        let state = test_state_with_payload_limit(128);
        let json_limit = state.config.json_payload_limit_bytes;
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state))
                .app_data(crate::api::json_config(json_limit))
                .configure(super::configure),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/inventory/labels")
            .set_json(serde_json::json!({
                "name": "oversized",
                "description": "x".repeat(1024),
            }))
            .to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
