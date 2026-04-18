use actix_web::{HttpRequest, HttpResponse, get, web};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    AppState, authz::actions, domain::pagination::PageRequest, errors::AppError,
    storage::StorageBackendKind,
};

use super::SystemListResponse;
use super::authz::{request as authz_request, require};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(health)
        .service(version)
        .service(status)
        .service(history);
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
        storage: state.reader.health().await?,
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
    require(
        &state,
        authz_request(
            &req,
            actions::system::STATUS_GET,
            actions::resource_kinds::SYSTEM,
            "status",
        ),
    )
    .await?;
    let modules = vec![
        "api", "domain", "services", "storage", "db", "authz", "tasks", "imports", "exports",
        "audit", "workers",
    ];

    Ok(HttpResponse::Ok().json(StatusResponse {
        phase: "phase-5-postgres-mvp-slice",
        storage_backend: state.reader.backend_kind(),
        storage_capabilities: state.reader.capabilities(),
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
    require(
        &state,
        authz_request(
            &req,
            actions::audit::HISTORY_LIST,
            actions::resource_kinds::AUDIT_HISTORY,
            "*",
        ),
    )
    .await?;
    let page = state.services.audit().list(&PageRequest::default()).await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.reader.backend_kind(),
    )))
}
