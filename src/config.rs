use std::{
    collections::BTreeSet,
    env, fs,
    net::{IpAddr, SocketAddr},
};

use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::ToSchema;

use crate::errors::AppError;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum StorageBackendSetting {
    #[default]
    Auto,
    Memory,
    Postgres,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AuthMode {
    #[default]
    None,
    Scoped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthProviderKind {
    Local,
    Ldap,
    Remote,
}

impl AuthProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ldap => "ldap",
            Self::Remote => "remote",
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct AuthProvidersDocument {
    #[serde(default)]
    pub providers: Vec<AuthProviderConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AuthProviderConfig {
    #[serde(rename = "scope")]
    pub name: String,
    #[serde(flatten)]
    pub backend: AuthProviderBackendConfig,
}

impl AuthProviderConfig {
    pub fn kind(&self) -> AuthProviderKind {
        self.backend.kind()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthProviderBackendConfig {
    Local {
        #[serde(default)]
        users: Vec<LocalUserConfig>,
    },
    Ldap {
        url: String,
        bind_dn: Option<String>,
        bind_password: Option<String>,
        #[serde(default = "default_ldap_connect_timeout_seconds")]
        connect_timeout_seconds: u64,
        #[serde(default = "default_ldap_operation_timeout_seconds")]
        operation_timeout_seconds: u64,
        user_base_dn: String,
        user_filter: String,
        #[serde(default = "default_ldap_search_scope")]
        user_scope: LdapSearchScope,
        #[serde(default = "default_ldap_username_attribute")]
        username_attribute: String,
        #[serde(default = "default_ldap_subject_attribute")]
        subject_attribute: String,
        display_name_attribute: Option<String>,
        email_attribute: Option<String>,
        #[serde(default)]
        group_attributes: Vec<String>,
        #[serde(default)]
        group_filters: Vec<String>,
        #[serde(default)]
        group_rules: Vec<GroupMappingRuleConfig>,
    },
    Remote {
        login_url: String,
        #[serde(default = "default_auth_timeout_ms")]
        timeout_ms: u64,
        default_service_name: Option<String>,
        jwt_issuer: String,
        jwt_audience: Option<String>,
        jwks_url: Option<String>,
        jwt_public_key_pem: Option<String>,
        jwt_hmac_secret: Option<String>,
        #[serde(default = "default_forward_username_claim")]
        username_claim: String,
        #[serde(default = "default_forward_groups_claim")]
        groups_claim: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LdapSearchScope {
    Base,
    One,
    #[default]
    Subtree,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupMappingRuleConfig {
    pub pattern: String,
    pub name: String,
}

impl AuthProviderBackendConfig {
    pub fn kind(&self) -> AuthProviderKind {
        match self {
            Self::Local { .. } => AuthProviderKind::Local,
            Self::Ldap { .. } => AuthProviderKind::Ldap,
            Self::Remote { .. } => AuthProviderKind::Remote,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LocalUserConfig {
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Clone)]
pub struct Config {
    pub listen: IpAddr,
    pub port: u16,
    pub workers: Option<usize>,
    pub json_logs: bool,
    pub json_payload_limit_bytes: usize,
    pub database_url: Option<String>,
    pub run_migrations: bool,
    pub storage_backend: StorageBackendSetting,
    pub treetop_url: Option<String>,
    pub treetop_timeout_ms: u64,
    pub allow_dev_authz_bypass: bool,
    pub allow_unsafe_urls: bool,
    pub auth_login_trust_proxy_headers: bool,
    pub auth_mode: AuthMode,
    pub auth_token_ttl_seconds: u64,
    pub auth_jwt_signing_key: Option<String>,
    pub auth_jwt_issuer: String,
    pub auth_config_path: Option<String>,
    pub auth_providers: Vec<AuthProviderConfig>,
    pub event_webhook_url: Option<String>,
    pub event_webhook_timeout_ms: u64,
    pub event_amqp_url: Option<String>,
    pub event_amqp_exchange: String,
    pub event_redis_url: Option<String>,
    pub event_redis_stream: String,
    pub dhcp_auto_v4_client_id: bool,
    pub dhcp_auto_v6_duid_ll: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1".parse().unwrap(),
            port: 8080,
            workers: None,
            json_logs: false,
            json_payload_limit_bytes: 1024 * 1024,
            database_url: None,
            run_migrations: true,
            storage_backend: StorageBackendSetting::Auto,
            treetop_url: None,
            treetop_timeout_ms: 1500,
            allow_dev_authz_bypass: false,
            allow_unsafe_urls: false,
            auth_login_trust_proxy_headers: false,
            auth_mode: AuthMode::None,
            auth_token_ttl_seconds: 3600,
            auth_jwt_signing_key: None,
            auth_jwt_issuer: "mreg-rust".to_string(),
            auth_config_path: None,
            auth_providers: Vec::new(),
            event_webhook_url: None,
            event_webhook_timeout_ms: 5000,
            event_amqp_url: None,
            event_amqp_exchange: "mreg.events".to_string(),
            event_redis_url: None,
            event_redis_stream: "mreg:events".to_string(),
            dhcp_auto_v4_client_id: false,
            dhcp_auto_v6_duid_ll: false,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        let auth_config_path = env::var("MREG_AUTH_CONFIG_PATH").ok();
        let auth_providers = match &auth_config_path {
            Some(path) => read_auth_providers_file(path)?,
            None => Vec::new(),
        };

        let listen = match env::var("MREG_LISTEN") {
            Ok(val) => val.parse::<IpAddr>().map_err(|_| {
                AppError::config(format!(
                    "MREG_LISTEN must be a valid IP address, got: {val}"
                ))
            })?,
            Err(_) => "127.0.0.1".parse().unwrap(),
        };

        let config = Self {
            listen,
            port: parse_or_default("MREG_PORT", 8080)?,
            workers: parse_optional("MREG_WORKERS")?,
            json_logs: parse_bool_or_default("MREG_JSON_LOGS", false)?,
            json_payload_limit_bytes: parse_or_default(
                "MREG_JSON_PAYLOAD_LIMIT_BYTES",
                1024 * 1024,
            )?,
            database_url: env::var("MREG_DATABASE_URL").ok(),
            run_migrations: parse_bool_or_default("MREG_RUN_MIGRATIONS", true)?,
            storage_backend: parse_storage_backend("MREG_STORAGE_BACKEND")?,
            treetop_url: env::var("MREG_TREETOP_URL").ok(),
            treetop_timeout_ms: parse_or_default("MREG_TREETOP_TIMEOUT_MS", 1500)?,
            allow_dev_authz_bypass: parse_bool_or_default("MREG_ALLOW_DEV_AUTHZ_BYPASS", false)?,
            allow_unsafe_urls: parse_bool_or_default("MREG_ALLOW_UNSAFE_URLS", false)?,
            auth_login_trust_proxy_headers: parse_bool_or_default(
                "MREG_AUTH_LOGIN_TRUST_PROXY_HEADERS",
                false,
            )?,
            auth_mode: parse_auth_mode("MREG_AUTH_MODE")?,
            auth_token_ttl_seconds: parse_or_default("MREG_AUTH_TOKEN_TTL_SECONDS", 3600)?,
            auth_jwt_signing_key: env::var("MREG_AUTH_JWT_SIGNING_KEY").ok(),
            auth_jwt_issuer: env::var("MREG_AUTH_JWT_ISSUER")
                .unwrap_or_else(|_| "mreg-rust".to_string()),
            auth_config_path,
            auth_providers,
            event_webhook_url: env::var("MREG_EVENT_WEBHOOK_URL").ok(),
            event_webhook_timeout_ms: parse_or_default("MREG_EVENT_WEBHOOK_TIMEOUT_MS", 5000)?,
            event_amqp_url: env::var("MREG_EVENT_AMQP_URL").ok(),
            event_amqp_exchange: env::var("MREG_EVENT_AMQP_EXCHANGE")
                .unwrap_or_else(|_| "mreg.events".to_string()),
            event_redis_url: env::var("MREG_EVENT_REDIS_URL").ok(),
            event_redis_stream: env::var("MREG_EVENT_REDIS_STREAM")
                .unwrap_or_else(|_| "mreg:events".to_string()),
            dhcp_auto_v4_client_id: parse_bool_or_default("MREG_DHCP_AUTO_V4_CLIENT_ID", false)?,
            dhcp_auto_v6_duid_ll: parse_bool_or_default("MREG_DHCP_AUTO_V6_DUID_LL", false)?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.listen, self.port)
    }

    pub fn trusts_identity_headers(&self) -> bool {
        matches!(self.auth_mode, AuthMode::None)
    }

    fn validate(&self) -> Result<(), AppError> {
        match self.auth_mode {
            AuthMode::None => {}
            AuthMode::Scoped => {
                require_present("MREG_AUTH_JWT_SIGNING_KEY", &self.auth_jwt_signing_key)?;
                if let Some(key) = &self.auth_jwt_signing_key
                    && key.len() < 32
                {
                    return Err(AppError::config(
                        "MREG_AUTH_JWT_SIGNING_KEY must be at least 32 bytes (256 bits) for HS256 security",
                    ));
                }
                if self.auth_providers.is_empty() {
                    return Err(AppError::config(
                        "scoped auth requires at least one configured auth scope",
                    ));
                }
                validate_providers(&self.auth_providers, self.allow_unsafe_urls)?;
            }
        }
        if let Some(url) = &self.treetop_url {
            validate_external_url(url, "MREG_TREETOP_URL", self.allow_unsafe_urls)?;
        }
        if let Some(url) = &self.event_webhook_url {
            validate_external_url(url, "MREG_EVENT_WEBHOOK_URL", self.allow_unsafe_urls)?;
        }
        Ok(())
    }
}

fn default_auth_timeout_ms() -> u64 {
    5000
}

fn default_ldap_connect_timeout_seconds() -> u64 {
    5
}

fn default_ldap_operation_timeout_seconds() -> u64 {
    10
}

fn default_ldap_search_scope() -> LdapSearchScope {
    LdapSearchScope::Subtree
}

fn default_ldap_username_attribute() -> String {
    "uid".to_string()
}

fn default_ldap_subject_attribute() -> String {
    "dn".to_string()
}

fn default_forward_username_claim() -> String {
    "sub".to_string()
}

fn default_forward_groups_claim() -> String {
    "groups".to_string()
}

/// Validates that an operator-configured URL uses HTTPS unless MREG_ALLOW_UNSAFE_URLS is set.
/// Prevents accidental plaintext fetches to sensitive endpoints (JWKS, webhooks, auth).
fn validate_external_url(url: &str, key: &str, allow_unsafe: bool) -> Result<(), AppError> {
    let parsed = Url::parse(url)
        .map_err(|_| AppError::config(format!("{key} is not a valid URL: {url}")))?;
    if !allow_unsafe && parsed.scheme() != "https" {
        return Err(AppError::config(format!(
            "{key} must use https (set MREG_ALLOW_UNSAFE_URLS=true to allow http in dev/test)"
        )));
    }
    Ok(())
}

fn validate_providers(
    scopes: &[AuthProviderConfig],
    allow_unsafe_urls: bool,
) -> Result<(), AppError> {
    let mut seen_scope_names = BTreeSet::new();
    for scope in scopes {
        if !is_valid_scope_name(&scope.name) {
            return Err(AppError::config(format!(
                "invalid auth scope name `{}`; use lowercase letters, digits, and hyphens",
                scope.name
            )));
        }
        if !seen_scope_names.insert(scope.name.clone()) {
            return Err(AppError::config(format!(
                "duplicate auth scope name `{}`",
                scope.name
            )));
        }
        match &scope.backend {
            AuthProviderBackendConfig::Local { users } => {
                let mut seen_usernames = BTreeSet::new();
                for user in users {
                    validate_raw_identity_component(&user.username, "local username")?;
                    if !seen_usernames.insert(user.username.clone()) {
                        return Err(AppError::config(format!(
                            "duplicate local username `{}` in scope `{}`",
                            user.username, scope.name
                        )));
                    }
                    if user.password_hash.trim().is_empty() {
                        return Err(AppError::config(format!(
                            "local user `{}` in scope `{}` is missing a password hash",
                            user.username, scope.name
                        )));
                    }
                    for group in &user.groups {
                        validate_raw_identity_component(group, "local group")?;
                    }
                }
            }
            AuthProviderBackendConfig::Ldap {
                url,
                bind_dn,
                bind_password,
                connect_timeout_seconds,
                operation_timeout_seconds,
                user_base_dn,
                user_filter,
                username_attribute,
                subject_attribute,
                display_name_attribute,
                email_attribute,
                group_attributes,
                group_filters,
                group_rules,
                ..
            } => {
                require_non_empty("ldap.url", url)?;
                validate_ldap_url(url)?;
                if *connect_timeout_seconds == 0 || *operation_timeout_seconds == 0 {
                    return Err(AppError::config("LDAP timeouts must be positive"));
                }
                require_non_empty("ldap.user_base_dn", user_base_dn)?;
                require_non_empty("ldap.user_filter", user_filter)?;
                if !user_filter.contains("{username}") {
                    return Err(AppError::config(
                        "ldap.user_filter must contain `{username}`",
                    ));
                }
                require_non_empty("ldap.username_attribute", username_attribute)?;
                require_non_empty("ldap.subject_attribute", subject_attribute)?;
                for (label, attribute) in [
                    ("ldap.display_name_attribute", display_name_attribute),
                    ("ldap.email_attribute", email_attribute),
                ] {
                    if let Some(attribute) = attribute {
                        require_non_empty(label, attribute)?;
                    }
                }
                for attribute in group_attributes {
                    require_non_empty("ldap.group_attributes", attribute)?;
                }
                match (bind_dn.as_deref(), bind_password.as_deref()) {
                    (Some(dn), Some(password)) => {
                        require_non_empty("ldap.bind_dn", dn)?;
                        require_non_empty("ldap.bind_password", password)?;
                    }
                    (None, None) => {}
                    _ => {
                        return Err(AppError::config(
                            "ldap.bind_dn and ldap.bind_password must be configured together",
                        ));
                    }
                }
                for filter in group_filters {
                    regex::Regex::new(filter).map_err(|error| {
                        AppError::config(format!("invalid LDAP group filter `{filter}`: {error}"))
                    })?;
                }
                for rule in group_rules {
                    regex::Regex::new(&rule.pattern).map_err(|error| {
                        AppError::config(format!(
                            "invalid LDAP group rule `{}`: {error}",
                            rule.pattern
                        ))
                    })?;
                    require_non_empty("ldap.group_rules.name", &rule.name)?;
                }
            }
            AuthProviderBackendConfig::Remote {
                login_url,
                jwt_issuer,
                jwks_url,
                jwt_public_key_pem,
                jwt_hmac_secret,
                username_claim,
                groups_claim,
                ..
            } => {
                require_non_empty("remote.login_url", login_url)?;
                validate_external_url(login_url, "remote.login_url", allow_unsafe_urls)?;
                require_non_empty("remote.jwt_issuer", jwt_issuer)?;
                require_non_empty("remote.username_claim", username_claim)?;
                require_non_empty("remote.groups_claim", groups_claim)?;
                if let Some(url) = jwks_url {
                    validate_external_url(url, "remote.jwks_url", allow_unsafe_urls)?;
                }
                let verification_sources = [
                    jwks_url.is_some(),
                    jwt_public_key_pem.is_some(),
                    jwt_hmac_secret.is_some(),
                ]
                .into_iter()
                .filter(|present| *present)
                .count();
                if verification_sources != 1 {
                    return Err(AppError::config(format!(
                        "remote auth scope `{}` requires exactly one of jwks_url, jwt_public_key_pem, or jwt_hmac_secret",
                        scope.name
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_raw_identity_component(value: &str, label: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::config(format!("{label} may not be empty")));
    }
    if value != value.trim() {
        return Err(AppError::config(format!(
            "{label} `{value}` may not have leading or trailing whitespace"
        )));
    }
    if value.contains(':') {
        return Err(AppError::config(format!(
            "{label} `{value}` may not contain `:`"
        )));
    }
    Ok(())
}

fn require_non_empty(label: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        Err(AppError::config(format!("{label} may not be empty")))
    } else {
        Ok(())
    }
}

fn is_valid_scope_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn validate_ldap_url(value: &str) -> Result<(), AppError> {
    let parsed = Url::parse(value)
        .map_err(|error| AppError::config(format!("invalid LDAP URL: {error}")))?;
    if parsed.host_str().is_none() {
        return Err(AppError::config("LDAP URL must include a host"));
    }
    match parsed.scheme() {
        "ldap" | "ldaps" => Ok(()),
        scheme => Err(AppError::config(format!(
            "LDAP URL must use ldap or ldaps, got `{scheme}`"
        ))),
    }
}

fn read_auth_providers_file(path: &str) -> Result<Vec<AuthProviderConfig>, AppError> {
    let raw = fs::read_to_string(path)
        .map_err(|error| AppError::config(format!("failed to read {path}: {error}")))?;
    let document = toml::from_str::<AuthProvidersDocument>(&raw)
        .map_err(|error| AppError::config(format!("failed to parse {path}: {error}")))?;
    Ok(document.providers)
}

fn parse_or_default<T>(key: &str, default: T) -> Result<T, AppError>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|error| AppError::config(format!("invalid value for {key}: {error}"))),
        Err(_) => Ok(default),
    }
}

fn parse_optional<T>(key: &str) -> Result<Option<T>, AppError>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    match env::var(key) {
        Ok(raw) => raw
            .parse::<T>()
            .map(Some)
            .map_err(|error| AppError::config(format!("invalid value for {key}: {error}"))),
        Err(_) => Ok(None),
    }
}

fn parse_bool_or_default(key: &str, default: bool) -> Result<bool, AppError> {
    match env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(AppError::config(format!(
                "invalid boolean for {key}: {raw}"
            ))),
        },
        Err(_) => Ok(default),
    }
}

fn parse_storage_backend(key: &str) -> Result<StorageBackendSetting, AppError> {
    match env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(StorageBackendSetting::Auto),
            "memory" => Ok(StorageBackendSetting::Memory),
            "postgres" => Ok(StorageBackendSetting::Postgres),
            _ => Err(AppError::config(format!(
                "invalid storage backend for {key}: {raw}; expected auto, memory, or postgres"
            ))),
        },
        Err(_) => Ok(StorageBackendSetting::Auto),
    }
}

fn parse_auth_mode(key: &str) -> Result<AuthMode, AppError> {
    match env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(AuthMode::None),
            "scoped" => Ok(AuthMode::Scoped),
            _ => Err(AppError::config(format!(
                "invalid auth mode for {key}: {raw}; expected none or scoped"
            ))),
        },
        Err(_) => Ok(AuthMode::None),
    }
}

fn require_present(key: &str, value: &Option<String>) -> Result<(), AppError> {
    if value.is_some() {
        Ok(())
    } else {
        Err(AppError::config(format!("missing required setting {key}")))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;
    use uuid::Uuid;

    /// A signing key of exactly 32 bytes, valid for HS256.
    const VALID_SIGNING_KEY: &str = "this_is_exactly_32_bytes_long_!!";

    fn temp_toml_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("mreg-rust-{name}-{}.toml", Uuid::new_v4()));
        path
    }

    fn scoped_config_with_local_scope() -> Config {
        Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some(VALID_SIGNING_KEY.to_string()),
            auth_providers: vec![AuthProviderConfig {
                name: "local".to_string(),
                backend: AuthProviderBackendConfig::Local { users: Vec::new() },
            }],
            ..Config::default()
        }
    }

    #[test]
    fn scoped_config_rejects_duplicate_scope_names() {
        let config = Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some(VALID_SIGNING_KEY.to_string()),
            auth_providers: vec![
                AuthProviderConfig {
                    name: "local".to_string(),
                    backend: AuthProviderBackendConfig::Local { users: Vec::new() },
                },
                AuthProviderConfig {
                    name: "local".to_string(),
                    backend: AuthProviderBackendConfig::Local { users: Vec::new() },
                },
            ],
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("duplicate"),
            "expected duplicate error, got: {err}"
        );
    }

    #[test]
    fn scoped_config_rejects_remote_scope_without_verifier() {
        let config = Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some(VALID_SIGNING_KEY.to_string()),
            auth_providers: vec![AuthProviderConfig {
                name: "remote".to_string(),
                backend: AuthProviderBackendConfig::Remote {
                    login_url: "https://auth.example/login".to_string(),
                    timeout_ms: 5000,
                    default_service_name: None,
                    jwt_issuer: "issuer".to_string(),
                    jwt_audience: None,
                    jwks_url: None,
                    jwt_public_key_pem: None,
                    jwt_hmac_secret: None,
                    username_claim: "sub".to_string(),
                    groups_claim: "groups".to_string(),
                },
            }],
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn scoped_config_rejects_short_signing_key() {
        let config = Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some("tooshort".to_string()),
            auth_providers: vec![AuthProviderConfig {
                name: "local".to_string(),
                backend: AuthProviderBackendConfig::Local { users: Vec::new() },
            }],
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("32 bytes"),
            "expected key length error, got: {err}"
        );
    }

    #[test]
    fn scoped_config_accepts_32_byte_signing_key() {
        assert!(scoped_config_with_local_scope().validate().is_ok());
    }

    #[test]
    fn validate_external_url_requires_https_by_default() {
        let err =
            validate_external_url("http://internal.example/jwks", "test_url", false).unwrap_err();
        assert!(
            err.to_string().contains("https"),
            "expected https error, got: {err}"
        );
    }

    #[test]
    fn validate_external_url_allows_http_when_unsafe_enabled() {
        assert!(
            validate_external_url("http://localhost/jwks", "test_url", true).is_ok(),
            "http should be allowed when allow_unsafe_urls = true"
        );
    }

    #[test]
    fn validate_external_url_accepts_https() {
        assert!(validate_external_url("https://auth.example/jwks", "test_url", false).is_ok());
    }

    #[test]
    fn validate_external_url_rejects_invalid_url() {
        assert!(validate_external_url("not a url", "test_url", false).is_err());
    }

    #[test]
    fn remote_scope_login_url_must_be_https() {
        let config = Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some(VALID_SIGNING_KEY.to_string()),
            auth_providers: vec![AuthProviderConfig {
                name: "remote".to_string(),
                backend: AuthProviderBackendConfig::Remote {
                    login_url: "http://auth.example/login".to_string(),
                    timeout_ms: 5000,
                    default_service_name: None,
                    jwt_issuer: "issuer".to_string(),
                    jwt_audience: None,
                    jwks_url: None,
                    jwt_public_key_pem: Some("-----BEGIN PUBLIC KEY-----\n...".to_string()),
                    jwt_hmac_secret: None,
                    username_claim: "sub".to_string(),
                    groups_claim: "groups".to_string(),
                },
            }],
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("https"),
            "expected https error for login_url, got: {err}"
        );
    }

    #[test]
    fn treetop_url_must_be_https() {
        let config = Config {
            treetop_url: Some("http://treetop.internal/api".to_string()),
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("https"),
            "expected https error for treetop_url, got: {err}"
        );
    }

    #[test]
    fn webhook_url_must_be_https() {
        let config = Config {
            event_webhook_url: Some("http://hooks.example/mreg".to_string()),
            ..Config::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains("https"),
            "expected https error for event_webhook_url, got: {err}"
        );
    }

    #[test]
    fn local_username_with_whitespace_is_rejected() {
        let err = validate_raw_identity_component(" alice", "test").unwrap_err();
        assert!(err.to_string().contains("whitespace"));
    }

    #[test]
    fn bind_addr_returns_correct_socket_addr() {
        let config = Config {
            listen: "0.0.0.0".parse().unwrap(),
            port: 9090,
            ..Config::default()
        };
        let addr = config.bind_addr();
        assert_eq!(addr.port(), 9090);
        assert_eq!(addr.ip().to_string(), "0.0.0.0");
    }

    #[test]
    fn trust_proxy_headers_defaults_to_false() {
        assert!(!Config::default().auth_login_trust_proxy_headers);
    }

    #[test]
    fn read_auth_providers_file_parses_local_provider_registry() {
        let path = temp_toml_path("auth-providers");
        fs::write(
            &path,
            r#"[[providers]]
scope = "local"
kind = "local"

[[providers.users]]
username = "admin"
password_hash = "$argon2id$v=19$m=19456,t=2,p=1$abc$def"
groups = ["ops", "net"]
"#,
        )
        .unwrap();

        let scopes = read_auth_providers_file(path.to_str().unwrap()).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].name, "local");
        match &scopes[0].backend {
            AuthProviderBackendConfig::Local { users } => {
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].username, "admin");
                assert_eq!(users[0].groups, vec!["ops", "net"]);
            }
            _ => panic!("expected local provider"),
        }
    }

    #[test]
    fn example_provider_registry_parses_with_ldap_group_mapping() {
        let document =
            toml::from_str::<AuthProvidersDocument>(include_str!("../auth-providers.example.toml"))
                .unwrap();
        let ldap = document
            .providers
            .iter()
            .find(|provider| provider.name == "ldap-primary")
            .unwrap();
        let actual = match &ldap.backend {
            AuthProviderBackendConfig::Ldap {
                group_filters,
                group_rules,
                ..
            } => (
                document.providers.len(),
                group_filters.clone(),
                group_rules.len(),
            ),
            _ => unreachable!("ldap-primary must be an LDAP provider"),
        };

        assert_eq!(
            actual,
            (3, vec!["^mreg-".to_string(), "^admin$".to_string()], 1)
        );
    }
}
