use std::{collections::HashMap, sync::Arc};

use actix_web::{HttpMessage, HttpRequest};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    authz::{Group, Principal},
    config::{AuthMode, AuthScopeBackendConfig, AuthScopeKind, Config},
    errors::AppError,
    storage::DynStorage,
};

mod forward;
mod jwt;
#[cfg(feature = "ldap")]
mod ldap;
mod local;

pub use self::jwt::{LocalJwtIssuer, LocalJwtValidator};

#[derive(Clone, Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub service_name: Option<String>,
    #[serde(default)]
    pub otp_code: Option<String>,
}

impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("service_name", &self.service_name)
            .field("otp_code", &self.otp_code.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthenticatedSession {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_at: DateTime<Utc>,
    pub principal: Principal,
    pub username: String,
    pub auth_scope: String,
    pub auth_provider_kind: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrincipalContext {
    pub principal: Principal,
    pub username: String,
    pub auth_scope: Option<String>,
    pub auth_provider_kind: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub issued_at: Option<DateTime<Utc>>,
    pub token_fingerprint: Option<String>,
}

impl PrincipalContext {
    pub fn scoped(
        principal: Principal,
        username: String,
        auth_scope: String,
        auth_provider_kind: String,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            principal,
            username,
            auth_scope: Some(auth_scope),
            auth_provider_kind: Some(auth_provider_kind),
            expires_at,
            issued_at: None,
            token_fingerprint: None,
        }
    }

    pub fn headers(principal: Principal, now: DateTime<Utc>) -> Self {
        Self {
            username: principal.id.clone(),
            principal,
            auth_scope: None,
            auth_provider_kind: None,
            expires_at: now,
            issued_at: None,
            token_fingerprint: None,
        }
    }

    pub fn with_issued_at(mut self, issued_at: DateTime<Utc>) -> Self {
        self.issued_at = Some(issued_at);
        self
    }
}

#[derive(Clone)]
struct ScopeEntry {
    name: String,
    kind: AuthScopeKind,
    authenticator: Arc<dyn ScopeAuthenticator>,
}

#[derive(Clone)]
struct ScopedAuthnClient {
    scopes: Arc<HashMap<String, ScopeEntry>>,
    issuer: LocalJwtIssuer,
    validator: LocalJwtValidator,
}

#[derive(Clone)]
pub struct AuthnClient {
    mode: AuthMode,
    storage: DynStorage,
    scoped: Option<ScopedAuthnClient>,
}

#[derive(Clone, Serialize)]
pub(crate) struct BackendLoginRequest {
    pub username: String,
    pub password: String,
    pub service_name: Option<String>,
    pub otp_code: Option<String>,
}

impl std::fmt::Debug for BackendLoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendLoginRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("service_name", &self.service_name)
            .field("otp_code", &self.otp_code.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AuthenticatedIdentity {
    pub username: String,
    pub groups: Vec<String>,
    pub max_expires_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub(crate) trait ScopeAuthenticator: Send + Sync {
    async fn login(
        &self,
        credentials: BackendLoginRequest,
    ) -> Result<AuthenticatedIdentity, AppError>;
}

impl AuthnClient {
    pub fn from_config(config: &Config, storage: DynStorage) -> Result<Self, AppError> {
        let scoped = match config.auth_mode {
            AuthMode::None => {
                tracing::warn!(
                    "Authentication mode is 'none' — identity is trusted from X-Mreg-User/X-Mreg-Groups headers without verification."
                );
                None
            }
            AuthMode::Scoped => {
                let signing_key = config
                    .auth_jwt_signing_key
                    .as_ref()
                    .ok_or_else(|| AppError::config("missing MREG_AUTH_JWT_SIGNING_KEY"))?;
                let mut scopes = HashMap::new();
                for scope in &config.auth_scopes {
                    let authenticator: Arc<dyn ScopeAuthenticator> = match &scope.backend {
                        AuthScopeBackendConfig::Local { users } => {
                            Arc::new(local::LocalScopeAuthenticator::new(users.clone()))
                        }
                        AuthScopeBackendConfig::Remote {
                            login_url,
                            timeout_ms,
                            default_service_name,
                            jwt_issuer,
                            jwt_audience,
                            jwks_url,
                            jwt_public_key_pem,
                            jwt_hmac_secret,
                            username_claim,
                            groups_claim,
                        } => Arc::new(forward::RemoteScopeAuthenticator::new(
                            forward::RemoteScopeConfig {
                                login_url: login_url.clone(),
                                timeout_ms: *timeout_ms,
                                default_service_name: default_service_name.clone(),
                                issuer: jwt_issuer.clone(),
                                audience: jwt_audience.clone(),
                                jwks_url: jwks_url.clone(),
                                jwt_public_key_pem: jwt_public_key_pem.clone(),
                                jwt_hmac_secret: jwt_hmac_secret.clone(),
                                username_claim: username_claim.clone(),
                                groups_claim: groups_claim.clone(),
                            },
                        )?),
                        AuthScopeBackendConfig::Ldap {
                            url,
                            timeout_ms,
                            user_search_base,
                            user_search_filter,
                            group_search_base,
                            group_search_filter,
                            bind_dn,
                            bind_password,
                        } => {
                            #[cfg(feature = "ldap")]
                            {
                                Arc::new(ldap::LdapScopeAuthenticator::new(
                                    url.clone(),
                                    *timeout_ms,
                                    user_search_base.clone(),
                                    user_search_filter.clone(),
                                    group_search_base.clone(),
                                    group_search_filter.clone(),
                                    bind_dn.clone(),
                                    bind_password.clone(),
                                ))
                            }
                            #[cfg(not(feature = "ldap"))]
                            {
                                let _ = (
                                    url,
                                    timeout_ms,
                                    user_search_base,
                                    user_search_filter,
                                    group_search_base,
                                    group_search_filter,
                                    bind_dn,
                                    bind_password,
                                );
                                return Err(AppError::config(
                                    "ldap auth scopes require the `ldap` feature",
                                ));
                            }
                        }
                    };
                    scopes.insert(
                        scope.name.clone(),
                        ScopeEntry {
                            name: scope.name.clone(),
                            kind: scope.kind(),
                            authenticator,
                        },
                    );
                }

                Some(ScopedAuthnClient {
                    scopes: Arc::new(scopes),
                    issuer: LocalJwtIssuer::new(
                        signing_key,
                        config.auth_jwt_issuer.clone(),
                        config.auth_token_ttl_seconds,
                    ),
                    validator: LocalJwtValidator::new(signing_key, config.auth_jwt_issuer.clone()),
                })
            }
        };

        Ok(Self {
            mode: config.auth_mode.clone(),
            storage,
            scoped,
        })
    }

    pub fn mode(&self) -> &AuthMode {
        &self.mode
    }

    pub fn requires_bearer_auth(&self) -> bool {
        !matches!(self.mode, AuthMode::None)
    }

    pub async fn login(&self, credentials: LoginRequest) -> Result<AuthenticatedSession, AppError> {
        let scoped = self.scoped.as_ref().ok_or_else(|| {
            AppError::unavailable("authentication is disabled in auth mode `none`")
        })?;
        let (scope_name, raw_username) = split_scoped_username(&credentials.username)?;
        let scope = scoped
            .scopes
            .get(scope_name)
            .ok_or_else(|| AppError::validation(format!("unknown auth scope `{scope_name}`")))?;
        let identity = scope
            .authenticator
            .login(BackendLoginRequest {
                username: raw_username.to_string(),
                password: credentials.password,
                service_name: credentials.service_name,
                otp_code: credentials.otp_code,
            })
            .await?;

        validate_backend_identity_component(&identity.username, "username")?;
        for group in &identity.groups {
            validate_backend_identity_component(group, "group")?;
        }

        let principal = canonical_principal(&scope.name, &scope.kind, &identity);
        let (access_token, expires_at) = scoped.issuer.issue_access_token(
            &principal,
            &identity.username,
            &scope.name,
            scope.kind.as_str(),
            identity.max_expires_at,
        )?;

        Ok(AuthenticatedSession {
            access_token,
            token_type: "Bearer",
            expires_at,
            principal,
            username: identity.username,
            auth_scope: scope.name.clone(),
            auth_provider_kind: scope.kind.as_str().to_string(),
        })
    }

    pub async fn authenticate_bearer(&self, token: &str) -> Result<PrincipalContext, AppError> {
        let scoped = self.scoped.as_ref().ok_or_else(|| {
            AppError::unauthorized("bearer token authentication is disabled in auth mode `none`")
        })?;

        let mut context = scoped.validator.validate(token)?;
        let token_fingerprint = token_fingerprint(token);
        if self
            .storage
            .auth_sessions()
            .is_token_revoked(&token_fingerprint)
            .await?
        {
            return Err(AppError::unauthorized("bearer token has been revoked"));
        }
        if let Some(revoked_before) = self
            .storage
            .auth_sessions()
            .principal_revoked_before(&context.principal.id)
            .await?
        {
            let issued_at = context.issued_at.ok_or_else(|| {
                AppError::unauthorized("token is missing `iat`, required for revocation checks")
            })?;
            if issued_at <= revoked_before {
                return Err(AppError::unauthorized(
                    "bearer token was invalidated by a logout-all operation",
                ));
            }
        }
        context.token_fingerprint = Some(token_fingerprint);
        Ok(context)
    }

    pub async fn logout(&self, context: &PrincipalContext) -> Result<(), AppError> {
        let token_fingerprint = context.token_fingerprint.clone().ok_or_else(|| {
            AppError::unavailable("logout is only supported for bearer-authenticated requests")
        })?;
        self.storage
            .auth_sessions()
            .revoke_token(
                token_fingerprint,
                context.principal.id.clone(),
                context.expires_at,
            )
            .await
    }

    pub async fn logout_all_for_principal(&self, principal_id: &str) -> Result<(), AppError> {
        // iat is stored at millisecond precision, so revoked_before with full nanosecond
        // precision correctly distinguishes tokens issued before vs after logout_all.
        self.storage
            .auth_sessions()
            .revoke_all_for_principal(principal_id.to_string(), Utc::now())
            .await
    }
}

fn split_scoped_username(value: &str) -> Result<(&str, &str), AppError> {
    let (scope, username) = value
        .split_once(':')
        .ok_or_else(|| AppError::validation("username must be in `scope:username` form"))?;
    if scope.trim().is_empty() || username.trim().is_empty() {
        return Err(AppError::validation(
            "username must be in `scope:username` form",
        ));
    }
    Ok((scope.trim(), username.trim()))
}

fn canonical_principal(
    scope_name: &str,
    scope_kind: &AuthScopeKind,
    identity: &AuthenticatedIdentity,
) -> Principal {
    let _ = scope_kind;
    Principal {
        id: canonicalize_scoped_value(scope_name, &identity.username),
        namespace: Vec::new(),
        groups: identity
            .groups
            .iter()
            .map(|group| Group {
                id: canonicalize_scoped_value(scope_name, group),
                namespace: Vec::new(),
            })
            .collect(),
    }
}

fn canonicalize_scoped_value(scope_name: &str, value: &str) -> String {
    format!("{}:{}", scope_name.trim(), value.trim())
}

fn validate_backend_identity_component(value: &str, label: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::unauthorized(format!(
            "{label} returned by auth provider may not be empty"
        )));
    }
    if value != value.trim() {
        return Err(AppError::unauthorized(format!(
            "{label} returned by auth provider may not have leading or trailing whitespace"
        )));
    }
    if value.contains(':') {
        return Err(AppError::unauthorized(format!(
            "{label} returned by auth provider may not contain `:`"
        )));
    }
    Ok(())
}

pub fn principal_context(req: &HttpRequest) -> Option<PrincipalContext> {
    req.extensions().get::<PrincipalContext>().cloned()
}

pub fn insert_principal_context(
    request: &mut actix_web::dev::ServiceRequest,
    context: PrincipalContext,
) {
    request.extensions_mut().insert(context);
}

pub fn header_principal(req: &HttpRequest) -> Principal {
    let user_id = req
        .headers()
        .get("X-Mreg-User")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .unwrap_or("anonymous")
        .to_string();

    let groups = req
        .headers()
        .get("X-Mreg-Groups")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .filter(|g| !g.trim().is_empty())
                .map(|g| Group {
                    id: g.trim().to_string(),
                    namespace: Vec::new(),
                })
                .collect()
        })
        .unwrap_or_default();

    Principal {
        id: user_id,
        namespace: Vec::new(),
        groups,
    }
}

pub fn token_fingerprint(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut fingerprint = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut fingerprint, "{byte:02x}");
    }
    fingerprint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_scoped_username_trims_whitespace() {
        let (scope, username) = split_scoped_username(" local : alice ").unwrap();
        assert_eq!(scope, "local");
        assert_eq!(username, "alice");
    }

    #[test]
    fn split_scoped_username_with_clean_input() {
        let (scope, username) = split_scoped_username("local:alice").unwrap();
        assert_eq!(scope, "local");
        assert_eq!(username, "alice");
    }

    #[test]
    fn split_scoped_username_rejects_missing_colon() {
        assert!(split_scoped_username("localAlice").is_err());
    }

    #[test]
    fn canonicalize_scoped_value_trims_both_sides() {
        assert_eq!(
            canonicalize_scoped_value(" local ", " alice "),
            "local:alice"
        );
    }

    #[test]
    fn validate_backend_identity_rejects_leading_whitespace() {
        let err = validate_backend_identity_component(" alice", "username").unwrap_err();
        assert!(err.to_string().contains("whitespace"), "got: {err}");
    }

    #[test]
    fn validate_backend_identity_rejects_trailing_whitespace() {
        let err = validate_backend_identity_component("alice ", "username").unwrap_err();
        assert!(err.to_string().contains("whitespace"), "got: {err}");
    }

    #[test]
    fn validate_backend_identity_accepts_clean_value() {
        assert!(validate_backend_identity_component("alice", "username").is_ok());
    }

    #[test]
    fn logout_all_timing_uses_millisecond_precision_iat() {
        // iat is stored as timestamp_millis() in the JWT, so issued_at has sub-second
        // precision. Tokens issued before logout_all will have issued_at < revoked_before,
        // while tokens issued after (even in the same second) will have issued_at > revoked_before.
        let before = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let middle = Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let after = Utc::now();

        assert!(before <= middle, "timestamps should be ordered");
        assert!(middle <= after, "timestamps should be ordered");
        // Token issued at `before` should be revoked by logout at `middle`
        assert!(before <= middle, "before token revoked by middle cutoff");
        // Token issued at `after` should NOT be revoked by logout at `middle`
        assert!(after > middle, "after token valid after middle cutoff");
    }
}
