use actix_governor::KeyExtractor;
use actix_web::{HttpRequest, HttpResponse, dev::ServiceRequest, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    authn::{self, PrincipalContext},
    authz::{actions, require_permission},
    errors::AppError,
};

/// Rate-limit key extractor for the login endpoint.
///
/// When `trust_proxy_headers` is true (enabled by MREG_AUTH_LOGIN_TRUST_PROXY_HEADERS),
/// the extractor reads the client IP from X-Forwarded-For then X-Real-IP, falling
/// back to peer_addr. Only enable this behind a trusted reverse proxy that strips
/// and rebuilds these headers; leaving it off when not behind such a proxy prevents
/// clients from spoofing headers to bypass the rate limit.
///
/// When false (the default), only peer_addr is used — safe in all deployments but
/// means all clients behind the same proxy share one rate-limit bucket.
#[derive(Clone, Debug)]
struct LoginRateLimitExtractor {
    trust_proxy_headers: bool,
}

impl KeyExtractor for LoginRateLimitExtractor {
    type Key = String;
    type KeyExtractionError = std::convert::Infallible;

    fn extract(&self, req: &ServiceRequest) -> Result<Self::Key, Self::KeyExtractionError> {
        if self.trust_proxy_headers {
            if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
                if let Ok(ip_str) = forwarded_for.to_str() {
                    if let Some(ip) = ip_str.split(',').next().map(str::trim) {
                        return Ok(ip.to_string());
                    }
                }
            }
            if let Some(real_ip) = req.headers().get("X-Real-IP") {
                if let Ok(ip_str) = real_ip.to_str() {
                    return Ok(ip_str.trim().to_string());
                }
            }
        }
        if let Some(addr) = req.peer_addr() {
            return Ok(addr.ip().to_string());
        }
        Ok("unknown".to_string())
    }
}

pub fn configure(cfg: &mut web::ServiceConfig, trust_proxy_headers: bool) {
    use actix_governor::{Governor, GovernorConfigBuilder};

    // Rate-limit the login endpoint: 5 req burst, then 1 req/s sustained (200 ms/token).
    // web::resource is used instead of web::scope("") to avoid the empty scope acting as
    // a catch-all that would block other routes from being matched.
    let login_conf = GovernorConfigBuilder::default()
        .key_extractor(LoginRateLimitExtractor {
            trust_proxy_headers,
        })
        .milliseconds_per_request(200)
        .burst_size(5)
        .finish()
        .expect("valid rate limit configuration");

    cfg.service(
        web::resource("/auth/login")
            .wrap(Governor::new(&login_conf))
            .route(web::post().to(login_handler)),
    )
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
pub struct GroupResponse {
    pub id: String,
    pub namespace: Vec<String>,
    pub key: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PrincipalResponse {
    pub id: String,
    pub namespace: Vec<String>,
    pub key: String,
    pub username: String,
    pub groups: Vec<GroupResponse>,
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
    #[serde(alias = "principal_id")]
    pub principal_key: String,
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
    login_handler(state, body).await
}

/// Login handler used by the rate-limited web::resource route.
async fn login_handler(
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
            &body.principal_key,
        )
        .build(),
    )
    .await?;
    state
        .authn
        .logout_all_for_principal(&body.principal_key)
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
        namespace: principal.namespace.clone(),
        key: principal.key(),
        username: username.to_string(),
        groups: principal
            .groups
            .iter()
            .map(|group| GroupResponse {
                id: group.id.clone(),
                namespace: group.namespace.clone(),
                key: group.key(),
            })
            .collect(),
    }
}
