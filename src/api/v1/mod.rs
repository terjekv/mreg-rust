pub mod attachment_community_assignments;
pub mod attachments;
pub mod auth;
mod authz;
pub mod bacnet_ids;
pub mod communities;
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
pub mod workflows;
pub mod zones;

use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, get, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    authz::{actions, require_permission},
    domain::{
        filters::RecordFilter,
        pagination::{PageRequest, SortDirection},
    },
    errors::AppError,
    services::records as record_service,
    storage::StorageBackendKind,
};

use self::authz::request as authz_request;

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
    fn from_page<T: Serialize>(
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
    cfg.service(health)
        .service(version)
        .service(status)
        .service(tasks)
        .service(imports)
        .service(export_templates)
        .service(export_runs)
        .service(record_types)
        .service(rrsets)
        .service(list_records_endpoint)
        .service(history)
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

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    status: &'static str,
    storage: crate::storage::StorageHealthReport,
    authn: &'static str,
    authz: &'static str,
}

/// Health check
#[utoipa::path(
    get,
    path = "/api/v1/system/health",
    responses(
        (status = 200, description = "Service health", body = HealthResponse)
    ),
    tag = "System"
)]
#[get("/system/health")]
pub(crate) async fn health(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(HealthResponse {
        status: "ok",
        storage: state.storage.health().await?,
        authn: match state.config.auth_mode {
            crate::config::AuthMode::None => "none",
            crate::config::AuthMode::Scoped => "scoped",
        },
        authz: if state.config.treetop_url.is_some() {
            "treetop"
        } else if state.config.allow_dev_authz_bypass {
            "bypass"
        } else {
            "deny"
        },
    }))
}

#[derive(Serialize, ToSchema)]
pub struct VersionResponse {
    service: String,
    version: String,
    git_sha: Option<String>,
    api_base: &'static str,
}

/// Service version information
#[utoipa::path(
    get,
    path = "/api/v1/system/version",
    responses(
        (status = 200, description = "Version information", body = VersionResponse)
    ),
    tag = "System"
)]
#[get("/system/version")]
pub(crate) async fn version(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(VersionResponse {
        service: state.build_info.package_name.to_string(),
        version: state.build_info.version.to_string(),
        git_sha: state.build_info.git_sha.map(str::to_string),
        api_base: "/api/v1",
    }))
}

#[derive(Serialize, ToSchema)]
pub struct StatusResponse {
    phase: &'static str,
    storage_backend: StorageBackendKind,
    storage_capabilities: crate::storage::StorageCapabilities,
    database_url_configured: bool,
    treetop_url_configured: bool,
    auth_mode: &'static str,
    run_migrations: bool,
    modules: Vec<&'static str>,
    generated_at: DateTime<Utc>,
}

/// Service status
#[utoipa::path(
    get,
    path = "/api/v1/system/status",
    responses(
        (status = 200, description = "Service status", body = StatusResponse)
    ),
    tag = "System"
)]
#[get("/system/status")]
pub(crate) async fn status(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::system::STATUS_GET,
            actions::resource_kinds::SYSTEM,
            "status",
        )
        .build(),
    )
    .await?;
    let modules = vec![
        "api", "domain", "services", "storage", "db", "authz", "tasks", "imports", "exports",
        "audit", "workers",
    ];

    Ok(HttpResponse::Ok().json(StatusResponse {
        phase: "phase-5-postgres-mvp-slice",
        storage_backend: state.storage.backend_kind(),
        storage_capabilities: state.storage.capabilities(),
        database_url_configured: state.config.database_url.is_some(),
        treetop_url_configured: state.config.treetop_url.is_some(),
        auth_mode: match state.config.auth_mode {
            crate::config::AuthMode::None => "none",
            crate::config::AuthMode::Scoped => "scoped",
        },
        run_migrations: state.config.run_migrations,
        modules,
        generated_at: Utc::now(),
    }))
}

/// List tasks
#[utoipa::path(
    get,
    path = "/api/v1/workflows/tasks",
    responses(
        (status = 200, description = "List of tasks", body = SystemListResponse)
    ),
    tag = "Workflows"
)]
#[get("/workflows/tasks")]
pub(crate) async fn tasks(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::task::LIST,
            actions::resource_kinds::TASK,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .tasks()
        .list_tasks(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
}

/// List imports
#[utoipa::path(
    get,
    path = "/api/v1/workflows/imports",
    responses(
        (status = 200, description = "List of import batches", body = SystemListResponse)
    ),
    tag = "Workflows"
)]
#[get("/workflows/imports")]
pub(crate) async fn imports(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::import_batch::LIST,
            actions::resource_kinds::IMPORT_BATCH,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .imports()
        .list_import_batches(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
}

/// List export templates
#[utoipa::path(
    get,
    path = "/api/v1/workflows/export-templates",
    responses(
        (status = 200, description = "List of export templates", body = SystemListResponse)
    ),
    tag = "Workflows"
)]
#[get("/workflows/export-templates")]
pub(crate) async fn export_templates(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::export_template::LIST,
            actions::resource_kinds::EXPORT_TEMPLATE,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .exports()
        .list_export_templates(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
}

/// List export runs
#[utoipa::path(
    get,
    path = "/api/v1/workflows/export-runs",
    responses(
        (status = 200, description = "List of export runs", body = SystemListResponse)
    ),
    tag = "Workflows"
)]
#[get("/workflows/export-runs")]
pub(crate) async fn export_runs(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::export_run::LIST,
            actions::resource_kinds::EXPORT_RUN,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .exports()
        .list_export_runs(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
}

/// List record types
#[utoipa::path(
    get,
    path = "/api/v1/dns/record-types",
    responses(
        (status = 200, description = "List of record types", body = SystemListResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/record-types")]
pub(crate) async fn record_types(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::record_type::LIST,
            actions::resource_kinds::RECORD_TYPE,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .records()
        .list_record_types(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
}

/// List RRsets
#[utoipa::path(
    get,
    path = "/api/v1/dns/rrsets",
    responses(
        (status = 200, description = "List of RRsets", body = SystemListResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/rrsets")]
pub(crate) async fn rrsets(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::rrset::LIST,
            actions::resource_kinds::RRSET,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .records()
        .list_rrsets(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
}

#[derive(Deserialize)]
pub struct ListRecordsQuery {
    // Pagination + sort
    after: Option<uuid::Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    // Operator-based filter params
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl ListRecordsQuery {
    fn into_parts(self) -> Result<(PageRequest, RecordFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let filter = RecordFilter::from_query_params(self.filters)?;
        Ok((page, filter))
    }
}

/// List records with optional filters
#[utoipa::path(
    get,
    path = "/api/v1/dns/records",
    responses(
        (status = 200, description = "List of records", body = SystemListResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/records")]
pub(crate) async fn list_records_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ListRecordsQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::record::LIST,
            actions::resource_kinds::RECORD,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = record_service::list_records(state.storage.records(), &page, &filter).await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        result,
        state.storage.backend_kind(),
    )))
}

/// List audit history events
#[utoipa::path(
    get,
    path = "/api/v1/system/history",
    responses(
        (status = 200, description = "List of history events", body = SystemListResponse)
    ),
    tag = "System"
)]
#[get("/system/history")]
pub(crate) async fn history(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            actions::audit::HISTORY_LIST,
            actions::resource_kinds::AUDIT_HISTORY,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .storage
        .audit()
        .list_events(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.storage.backend_kind(),
    )))
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
        storage::build_storage,
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
        let authz = AuthorizerClient::from_config(&config);

        AppState {
            config: Arc::new(config),
            build_info: BuildInfo::current(),
            storage,
            authn,
            authz,
            events: EventSinkClient::noop(),
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
