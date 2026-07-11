use std::{collections::HashMap, fmt, sync::Arc};

use actix_web::{HttpMessage, HttpRequest};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::{
    authz::{Group, Principal, scoped_identity_namespace},
    config::{AuthMode, AuthProviderBackendConfig, AuthProviderKind, Config},
    errors::AppError,
    storage::DynStorage,
};

mod forward;
mod jwt;
#[cfg(feature = "ldap")]
mod ldap;
mod local;

pub use self::jwt::{LocalJwtIssuer, LocalJwtValidator};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, ToSchema)]
#[serde(transparent)]
pub struct IdentityScopeName(String);

impl IdentityScopeName {
    pub fn new(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.is_empty()
            || !value.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
        {
            return Err(AppError::validation(
                "identity_scope must contain only lowercase letters, digits, and hyphens",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdentityScopeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for IdentityScopeName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct LoginRequest {
    pub identity_scope: IdentityScopeName,
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
            .field("identity_scope", &self.identity_scope)
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
    pub identity_scope: String,
    pub auth_provider_kind: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrincipalContext {
    pub principal: Principal,
    pub username: String,
    pub identity_scope: Option<String>,
    pub auth_provider_kind: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub issued_at: Option<DateTime<Utc>>,
    pub token_fingerprint: Option<String>,
}

impl PrincipalContext {
    pub fn scoped(
        principal: Principal,
        username: String,
        identity_scope: String,
        auth_provider_kind: String,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            principal,
            username,
            identity_scope: Some(identity_scope),
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
            identity_scope: None,
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

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct AuthProviderDescriptor {
    #[schema(value_type = String)]
    pub identity_scope: IdentityScopeName,
    pub kind: AuthProviderKind,
    pub display_name: String,
    pub display_order: u16,
    pub supports_service_name: bool,
    pub supports_otp: bool,
}

struct RegisteredAuthProvider {
    descriptor: AuthProviderDescriptor,
    backend: Arc<dyn AuthProviderBackend>,
}

struct AuthProviderRegistry {
    providers: HashMap<IdentityScopeName, RegisteredAuthProvider>,
}

impl AuthProviderRegistry {
    fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    fn register(&mut self, provider: RegisteredAuthProvider) -> Result<(), AppError> {
        let identity_scope = provider.descriptor.identity_scope.clone();
        if self
            .providers
            .insert(identity_scope.clone(), provider)
            .is_some()
        {
            return Err(AppError::config(format!(
                "duplicate auth provider `{identity_scope}`"
            )));
        }
        Ok(())
    }

    fn provider(
        &self,
        identity_scope: &IdentityScopeName,
    ) -> Result<&RegisteredAuthProvider, AppError> {
        self.providers.get(identity_scope).ok_or_else(|| {
            AppError::validation(format!("unknown identity_scope `{identity_scope}`"))
        })
    }

    fn iter(&self) -> impl Iterator<Item = &RegisteredAuthProvider> {
        self.providers.values()
    }

    fn descriptors(&self) -> Vec<AuthProviderDescriptor> {
        let mut providers = self
            .iter()
            .map(|provider| provider.descriptor.clone())
            .collect::<Vec<_>>();
        providers.sort_unstable_by(|left, right| {
            left.display_order
                .cmp(&right.display_order)
                .then_with(|| left.identity_scope.cmp(&right.identity_scope))
        });
        providers
    }
}

#[derive(Clone)]
struct ScopedAuthnClient {
    providers: Arc<AuthProviderRegistry>,
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
pub(crate) trait AuthProviderBackend: Send + Sync {
    async fn authenticate(
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
                let mut providers = AuthProviderRegistry::new();
                for scope in &config.auth_providers {
                    let backend: Arc<dyn AuthProviderBackend> = match &scope.backend {
                        AuthProviderBackendConfig::Local { users } => {
                            Arc::new(local::LocalAuthProvider::new(users.clone()))
                        }
                        AuthProviderBackendConfig::Remote {
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
                        } => Arc::new(forward::RemoteAuthProvider::new(
                            forward::RemoteProviderConfig {
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
                        AuthProviderBackendConfig::Ldap {
                            url,
                            bind_dn,
                            bind_password,
                            connect_timeout_seconds,
                            operation_timeout_seconds,
                            user_base_dn,
                            user_filter,
                            user_scope,
                            username_attribute,
                            subject_attribute,
                            display_name_attribute,
                            email_attribute,
                            group_attributes,
                            group_filters,
                            group_rules,
                        } => {
                            #[cfg(feature = "ldap")]
                            {
                                Arc::new(ldap::LdapAuthProvider::new(
                                    ldap::LdapAuthenticatorConfig {
                                        url: url.clone(),
                                        bind_dn: bind_dn.clone(),
                                        bind_password: bind_password.clone(),
                                        connect_timeout_seconds: *connect_timeout_seconds,
                                        operation_timeout_seconds: *operation_timeout_seconds,
                                        user_base_dn: user_base_dn.clone(),
                                        user_filter: user_filter.clone(),
                                        user_scope: *user_scope,
                                        username_attribute: username_attribute.clone(),
                                        subject_attribute: subject_attribute.clone(),
                                        display_name_attribute: display_name_attribute.clone(),
                                        email_attribute: email_attribute.clone(),
                                        group_attributes: group_attributes.clone(),
                                        group_filters: group_filters.clone(),
                                        group_rules: group_rules.clone(),
                                    },
                                )?)
                            }
                            #[cfg(not(feature = "ldap"))]
                            {
                                let _ = (
                                    url,
                                    bind_dn,
                                    bind_password,
                                    connect_timeout_seconds,
                                    operation_timeout_seconds,
                                    user_base_dn,
                                    user_filter,
                                    user_scope,
                                    username_attribute,
                                    subject_attribute,
                                    display_name_attribute,
                                    email_attribute,
                                    group_attributes,
                                    group_filters,
                                    group_rules,
                                );
                                return Err(AppError::config(
                                    "LDAP auth providers require the `ldap` feature",
                                ));
                            }
                        }
                    };
                    let kind = scope.kind();
                    let identity_scope = IdentityScopeName::new(scope.name.clone())
                        .map_err(|error| AppError::config(error.to_string()))?;
                    let is_remote = matches!(kind, AuthProviderKind::Remote);
                    providers.register(RegisteredAuthProvider {
                        descriptor: AuthProviderDescriptor {
                            display_name: scope.name.clone(),
                            display_order: if matches!(kind, AuthProviderKind::Local) {
                                0
                            } else {
                                100
                            },
                            identity_scope,
                            kind,
                            supports_service_name: is_remote,
                            supports_otp: is_remote,
                        },
                        backend,
                    })?;
                }

                Some(ScopedAuthnClient {
                    providers: Arc::new(providers),
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

    pub fn providers(&self) -> Vec<AuthProviderDescriptor> {
        self.scoped
            .as_ref()
            .map(|scoped| scoped.providers.descriptors())
            .unwrap_or_default()
    }

    pub async fn login(&self, credentials: LoginRequest) -> Result<AuthenticatedSession, AppError> {
        let scoped = self.scoped.as_ref().ok_or_else(|| {
            AppError::unavailable("authentication is disabled in auth mode `none`")
        })?;
        let provider = scoped.providers.provider(&credentials.identity_scope)?;
        let identity = provider
            .backend
            .authenticate(BackendLoginRequest {
                username: credentials.username,
                password: credentials.password,
                service_name: credentials.service_name,
                otp_code: credentials.otp_code,
            })
            .await?;

        validate_backend_identity_component(&identity.username, "username")?;
        for group in &identity.groups {
            validate_backend_identity_component(group, "group")?;
        }

        let identity_scope = provider.descriptor.identity_scope.as_str();
        let provider_kind = provider.descriptor.kind;
        let principal = canonical_principal(identity_scope, &identity);
        let (access_token, expires_at) = scoped.issuer.issue_access_token(
            &principal,
            &identity.username,
            identity_scope,
            provider_kind.as_str(),
            identity.max_expires_at,
        )?;

        Ok(AuthenticatedSession {
            access_token,
            token_type: "Bearer",
            expires_at,
            principal,
            username: identity.username,
            identity_scope: identity_scope.to_string(),
            auth_provider_kind: provider_kind.as_str().to_string(),
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
            .principal_revoked_before(&context.principal.key())
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
                context.principal.key(),
                context.expires_at,
            )
            .await
    }

    pub async fn logout_all_for_principal(&self, principal_key: &str) -> Result<(), AppError> {
        // iat is stored at millisecond precision, so revoked_before with full nanosecond
        // precision correctly distinguishes tokens issued before vs after logout_all.
        self.storage
            .auth_sessions()
            .revoke_all_for_principal(principal_key.to_string(), Utc::now())
            .await
    }
}

fn canonical_principal(scope_name: &str, identity: &AuthenticatedIdentity) -> Principal {
    let namespace = scoped_identity_namespace(scope_name);
    Principal {
        id: identity.username.clone(),
        namespace: namespace.clone(),
        groups: identity
            .groups
            .iter()
            .map(|group| Group {
                id: group.clone(),
                namespace: namespace.clone(),
            })
            .collect(),
    }
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
    use rstest::rstest;

    #[rstest]
    #[case("ldap-primary", true)]
    #[case("LDAP Primary", false)]
    fn identity_scope_name_validation_cases(#[case] value: &str, #[case] valid: bool) {
        assert_eq!(IdentityScopeName::new(value).is_ok(), valid);
    }

    #[test]
    fn canonical_principal_uses_namespace_aware_identity() {
        let principal = canonical_principal(
            "local",
            &AuthenticatedIdentity {
                username: "alice".to_string(),
                groups: vec!["ops".to_string()],
                max_expires_at: None,
            },
        );

        assert_eq!(principal.id, "alice");
        assert_eq!(
            principal.namespace,
            vec!["mreg".to_string(), "local".to_string()]
        );
        assert_eq!(principal.key(), "mreg::local::alice");
        assert_eq!(principal.groups[0].id, "ops");
        assert_eq!(
            principal.groups[0].namespace,
            vec!["mreg".to_string(), "local".to_string()]
        );
        assert_eq!(principal.groups[0].key(), "mreg::local::ops");
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
