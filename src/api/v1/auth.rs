use actix_web::{HttpRequest, HttpResponse, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    authn::{self, PrincipalContext},
    authz::{actions, require_permission},
    errors::AppError,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(login)
        .service(me)
        .service(logout)
        .service(logout_all);
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub service_name: Option<String>,
    pub otp_code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PrincipalResponse {
    pub id: String,
    pub username: String,
    pub groups: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: DateTime<Utc>,
    pub principal: PrincipalResponse,
    pub auth_scope: String,
    pub auth_provider_kind: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub principal: PrincipalResponse,
    pub auth_scope: Option<String>,
    pub auth_provider_kind: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogoutAllRequest {
    pub principal_id: String,
}

/// Login and obtain a bearer token.
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Authenticated session", body = LoginResponse),
        (status = 401, description = "Invalid credentials"),
        (status = 503, description = "Authentication backend unavailable")
    ),
    tag = "Authentication"
)]
#[post("/auth/login")]
pub(crate) async fn login(
    state: web::Data<AppState>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, AppError> {
    let session = state
        .authn
        .login(authn::LoginRequest {
            username: body.username.clone(),
            password: body.password.clone(),
            service_name: body.service_name.clone(),
            otp_code: body.otp_code.clone(),
        })
        .await?;
    Ok(HttpResponse::Ok().json(LoginResponse {
        access_token: session.access_token,
        token_type: session.token_type.to_string(),
        expires_at: session.expires_at,
        principal: principal_response(&session.principal, &session.username),
        auth_scope: session.auth_scope,
        auth_provider_kind: session.auth_provider_kind,
    }))
}

/// Show the authenticated principal for the current request.
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    responses(
        (status = 200, description = "Current authenticated principal", body = MeResponse),
        (status = 401, description = "Authentication required")
    ),
    tag = "Authentication"
)]
#[get("/auth/me")]
pub(crate) async fn me(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let (context, expires_at) = current_principal_context(&req, &state)?;
    Ok(HttpResponse::Ok().json(MeResponse {
        principal: principal_response(&context.principal, &context.username),
        auth_scope: context.auth_scope.clone(),
        auth_provider_kind: context.auth_provider_kind.clone(),
        expires_at,
    }))
}

/// Revoke the current bearer token.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    responses(
        (status = 204, description = "Current token revoked"),
        (status = 401, description = "Authentication required"),
        (status = 503, description = "Logout unavailable in header-trust mode")
    ),
    tag = "Authentication"
)]
#[post("/auth/logout")]
pub(crate) async fn logout(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    let (context, _) = current_principal_context(&req, &state)?;
    state.authn.logout(&context).await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Revoke all tokens for a principal.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout-all",
    request_body = LogoutAllRequest,
    responses(
        (status = 204, description = "All tokens revoked for the principal"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Permission denied")
    ),
    tag = "Authentication"
)]
#[post("/auth/logout-all")]
pub(crate) async fn logout_all(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<LogoutAllRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        super::authz::request(
            &req,
            actions::auth_session::LOGOUT_ALL,
            actions::resource_kinds::AUTH_SESSION,
            &body.principal_id,
        )
        .build(),
    )
    .await?;
    state
        .authn
        .logout_all_for_principal(&body.principal_id)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

fn current_principal_context(
    req: &HttpRequest,
    state: &AppState,
) -> Result<(PrincipalContext, Option<DateTime<Utc>>), AppError> {
    if let Some(context) = authn::principal_context(req) {
        let expires_at = context.expires_at;
        return Ok((context, Some(expires_at)));
    }

    if state.config.trusts_identity_headers() {
        return Ok((
            PrincipalContext::headers(authn::header_principal(req), Utc::now()),
            None,
        ));
    }

    Err(AppError::unauthorized("authentication required"))
}

fn principal_response(principal: &crate::authz::Principal, username: &str) -> PrincipalResponse {
    PrincipalResponse {
        id: principal.id.clone(),
        username: username.to_string(),
        groups: principal
            .groups
            .iter()
            .map(|group| group.id.clone())
            .collect(),
    }
}
