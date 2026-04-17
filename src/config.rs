use std::{collections::BTreeSet, env, fs, net::SocketAddr};

use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthScopeKind {
    Local,
    Ldap,
    Remote,
}

impl AuthScopeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Ldap => "ldap",
            Self::Remote => "remote",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthScopesDocument {
    pub scopes: Vec<AuthScopeConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthScopeConfig {
    pub name: String,
    #[serde(flatten)]
    pub backend: AuthScopeBackendConfig,
}

impl AuthScopeConfig {
    pub fn kind(&self) -> AuthScopeKind {
        self.backend.kind()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthScopeBackendConfig {
    Local {
        #[serde(default)]
        users: Vec<LocalUserConfig>,
    },
    Ldap {
        url: String,
        #[serde(default = "default_auth_timeout_ms")]
        timeout_ms: u64,
        user_search_base: String,
        user_search_filter: String,
        group_search_base: String,
        group_search_filter: String,
        bind_dn: Option<String>,
        bind_password: Option<String>,
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

impl AuthScopeBackendConfig {
    pub fn kind(&self) -> AuthScopeKind {
        match self {
            Self::Local { .. } => AuthScopeKind::Local,
            Self::Ldap { .. } => AuthScopeKind::Ldap,
            Self::Remote { .. } => AuthScopeKind::Remote,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalUserConfig {
    pub username: String,
    pub password_hash: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: String,
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
    pub auth_mode: AuthMode,
    pub auth_token_ttl_seconds: u64,
    pub auth_jwt_signing_key: Option<String>,
    pub auth_jwt_issuer: String,
    pub auth_scopes_file: Option<String>,
    pub auth_scopes: Vec<AuthScopeConfig>,
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
            listen: "127.0.0.1".to_string(),
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
            auth_mode: AuthMode::None,
            auth_token_ttl_seconds: 3600,
            auth_jwt_signing_key: None,
            auth_jwt_issuer: "mreg-rust".to_string(),
            auth_scopes_file: None,
            auth_scopes: Vec::new(),
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
        let auth_scopes_file = env::var("MREG_AUTH_SCOPES_FILE").ok();
        let auth_scopes = match &auth_scopes_file {
            Some(path) => read_auth_scopes_file(path)?,
            None => Vec::new(),
        };

        let config = Self {
            listen: env::var("MREG_LISTEN").unwrap_or_else(|_| "127.0.0.1".to_string()),
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
            auth_mode: parse_auth_mode("MREG_AUTH_MODE")?,
            auth_token_ttl_seconds: parse_or_default("MREG_AUTH_TOKEN_TTL_SECONDS", 3600)?,
            auth_jwt_signing_key: env::var("MREG_AUTH_JWT_SIGNING_KEY").ok(),
            auth_jwt_issuer: env::var("MREG_AUTH_JWT_ISSUER")
                .unwrap_or_else(|_| "mreg-rust".to_string()),
            auth_scopes_file,
            auth_scopes,
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
        format!("{}:{}", self.listen, self.port)
            .parse()
            .expect("validated listen/port configuration")
    }

    pub fn trusts_identity_headers(&self) -> bool {
        matches!(self.auth_mode, AuthMode::None)
    }

    fn validate(&self) -> Result<(), AppError> {
        match self.auth_mode {
            AuthMode::None => Ok(()),
            AuthMode::Scoped => {
                require_present("MREG_AUTH_JWT_SIGNING_KEY", &self.auth_jwt_signing_key)?;
                if self.auth_scopes.is_empty() {
                    return Err(AppError::config(
                        "scoped auth requires at least one configured auth scope",
                    ));
                }
                validate_scopes(&self.auth_scopes)
            }
        }
    }
}

fn default_auth_timeout_ms() -> u64 {
    5000
}

fn default_forward_username_claim() -> String {
    "sub".to_string()
}

fn default_forward_groups_claim() -> String {
    "groups".to_string()
}

fn validate_scopes(scopes: &[AuthScopeConfig]) -> Result<(), AppError> {
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
            AuthScopeBackendConfig::Local { users } => {
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
            AuthScopeBackendConfig::Ldap {
                url,
                user_search_base,
                user_search_filter,
                group_search_base,
                group_search_filter,
                ..
            } => {
                require_non_empty("ldap.url", url)?;
                require_non_empty("ldap.user_search_base", user_search_base)?;
                require_non_empty("ldap.user_search_filter", user_search_filter)?;
                require_non_empty("ldap.group_search_base", group_search_base)?;
                require_non_empty("ldap.group_search_filter", group_search_filter)?;
            }
            AuthScopeBackendConfig::Remote {
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
                require_non_empty("remote.jwt_issuer", jwt_issuer)?;
                require_non_empty("remote.username_claim", username_claim)?;
                require_non_empty("remote.groups_claim", groups_claim)?;
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

fn read_auth_scopes_file(path: &str) -> Result<Vec<AuthScopeConfig>, AppError> {
    let raw = fs::read_to_string(path)
        .map_err(|error| AppError::config(format!("failed to read {path}: {error}")))?;
    let document = serde_json::from_str::<AuthScopesDocument>(&raw)
        .map_err(|error| AppError::config(format!("failed to parse {path}: {error}")))?;
    Ok(document.scopes)
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

    fn temp_json_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("mreg-rust-{name}-{}.json", Uuid::new_v4()));
        path
    }

    #[test]
    fn scoped_config_rejects_duplicate_scope_names() {
        let config = Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some("secret".to_string()),
            auth_scopes: vec![
                AuthScopeConfig {
                    name: "local".to_string(),
                    backend: AuthScopeBackendConfig::Local { users: Vec::new() },
                },
                AuthScopeConfig {
                    name: "local".to_string(),
                    backend: AuthScopeBackendConfig::Local { users: Vec::new() },
                },
            ],
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn scoped_config_rejects_remote_scope_without_verifier() {
        let config = Config {
            auth_mode: AuthMode::Scoped,
            auth_jwt_signing_key: Some("secret".to_string()),
            auth_scopes: vec![AuthScopeConfig {
                name: "remote".to_string(),
                backend: AuthScopeBackendConfig::Remote {
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
    fn read_auth_scopes_file_parses_local_scope_registry() {
        let path = temp_json_path("auth-scopes");
        fs::write(
            &path,
            r#"{
  "scopes": [
    {
      "name": "local",
      "kind": "local",
      "users": [
        {
          "username": "admin",
          "password_hash": "$argon2id$v=19$m=19456,t=2,p=1$abc$def",
          "groups": ["ops", "net"]
        }
      ]
    }
  ]
}"#,
        )
        .unwrap();

        let scopes = read_auth_scopes_file(path.to_str().unwrap()).unwrap();
        let _ = fs::remove_file(&path);

        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].name, "local");
        match &scopes[0].backend {
            AuthScopeBackendConfig::Local { users } => {
                assert_eq!(users.len(), 1);
                assert_eq!(users[0].username, "admin");
                assert_eq!(users[0].groups, vec!["ops", "net"]);
            }
            other => panic!("expected local scope, got {other:?}"),
        }
    }
}
