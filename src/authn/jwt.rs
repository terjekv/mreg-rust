use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
    jwk::JwkSet,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::{
    authz::{Group, Principal},
    errors::AppError,
};

use super::{AuthenticatedIdentity, PrincipalContext};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalJwtGroupClaim {
    pub id: String,
    pub namespace: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalJwtClaims {
    pub sub: String,
    pub principal_id: String,
    pub principal_namespace: Vec<String>,
    pub username: String,
    pub groups: Vec<LocalJwtGroupClaim>,
    pub auth_scope: String,
    pub auth_provider_kind: String,
    pub iat: i64,
    pub exp: i64,
    pub iss: String,
}

#[derive(Clone)]
pub struct LocalJwtIssuer {
    encoding_key: EncodingKey,
    issuer: String,
    ttl: Duration,
}

impl LocalJwtIssuer {
    pub fn new(signing_key: &str, issuer: impl Into<String>, ttl_seconds: u64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(signing_key.as_bytes()),
            issuer: issuer.into(),
            ttl: Duration::seconds(ttl_seconds as i64),
        }
    }

    pub fn issue_access_token(
        &self,
        principal: &Principal,
        raw_username: &str,
        auth_scope: &str,
        auth_provider_kind: &str,
        max_expires_at: Option<DateTime<Utc>>,
    ) -> Result<(String, DateTime<Utc>), AppError> {
        let now = Utc::now();
        let local_expires_at = now + self.ttl;
        let expires_at = match max_expires_at {
            Some(external_expires_at) if external_expires_at < local_expires_at => {
                external_expires_at
            }
            _ => local_expires_at,
        };
        let claims = LocalJwtClaims {
            sub: principal.key(),
            principal_id: principal.id.clone(),
            principal_namespace: principal.namespace.clone(),
            username: raw_username.to_string(),
            groups: principal
                .groups
                .iter()
                .map(|group| LocalJwtGroupClaim {
                    id: group.id.clone(),
                    namespace: group.namespace.clone(),
                })
                .collect(),
            auth_scope: auth_scope.to_string(),
            auth_provider_kind: auth_provider_kind.to_string(),
            // Store iat as milliseconds for sub-second revocation precision.
            // This is non-standard (RFC 7519 uses seconds) but allows logout_all
            // to correctly distinguish tokens issued before vs after the cutoff
            // within the same second boundary.
            iat: now.timestamp_millis(),
            exp: expires_at.timestamp(),
            iss: self.issuer.clone(),
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(AppError::internal)?;
        Ok((token, expires_at))
    }
}

#[derive(Clone)]
pub struct LocalJwtValidator {
    decoding_key: DecodingKey,
    issuer: String,
}

impl LocalJwtValidator {
    pub fn new(signing_key: &str, issuer: impl Into<String>) -> Self {
        Self {
            decoding_key: DecodingKey::from_secret(signing_key.as_bytes()),
            issuer: issuer.into(),
        }
    }

    pub fn validate(&self, token: &str) -> Result<PrincipalContext, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        let data = decode::<LocalJwtClaims>(token, &self.decoding_key, &validation)
            .map_err(jwt_error_to_unauthorized)?;
        let expires_at = Utc
            .timestamp_opt(data.claims.exp, 0)
            .single()
            .ok_or_else(|| AppError::unauthorized("invalid token expiry"))?;
        let issued_at = DateTime::from_timestamp_millis(data.claims.iat)
            .ok_or_else(|| AppError::unauthorized("invalid token issue time"))?;
        let principal = Principal {
            id: data.claims.principal_id,
            namespace: data.claims.principal_namespace,
            groups: data
                .claims
                .groups
                .into_iter()
                .map(|group| Group {
                    id: group.id,
                    namespace: group.namespace,
                })
                .collect(),
        };
        if data.claims.sub != principal.key() {
            return Err(AppError::unauthorized(
                "token subject does not match principal identity",
            ));
        }
        Ok(PrincipalContext::scoped(
            principal,
            data.claims.username,
            data.claims.auth_scope,
            data.claims.auth_provider_kind,
            expires_at,
        )
        .with_issued_at(issued_at))
    }
}

#[derive(Clone)]
pub struct ForwardJwtValidator {
    username_claim: String,
    groups_claim: String,
    issuer: String,
    audience: Option<String>,
    verifier: ForwardVerifier,
}

#[derive(Clone)]
enum ForwardVerifier {
    Hmac { secret: String },
    Pem { pem: String },
    Jwks(JwksVerifier),
}

/// JWKS cache TTL: keys are re-fetched after this duration.
const JWKS_CACHE_TTL_SECS: i64 = 300; // 5 minutes

#[derive(Clone)]
struct JwksCacheEntry {
    set: JwkSet,
    fetched_at: DateTime<Utc>,
}

#[derive(Clone)]
struct JwksVerifier {
    url: String,
    client: reqwest::Client,
    cache: Arc<RwLock<Option<JwksCacheEntry>>>,
}

pub struct ForwardJwtValidatorConfig {
    pub issuer: String,
    pub audience: Option<String>,
    pub jwks_url: Option<String>,
    pub jwt_public_key_pem: Option<String>,
    pub jwt_hmac_secret: Option<String>,
    pub username_claim: String,
    pub groups_claim: String,
    pub timeout_ms: u64,
}

impl ForwardJwtValidator {
    pub fn new(config: ForwardJwtValidatorConfig) -> Result<Self, AppError> {
        let ForwardJwtValidatorConfig {
            issuer,
            audience,
            jwks_url,
            jwt_public_key_pem,
            jwt_hmac_secret,
            username_claim,
            groups_claim,
            timeout_ms,
        } = config;
        let verifier = if let Some(secret) = jwt_hmac_secret {
            ForwardVerifier::Hmac { secret }
        } else if let Some(pem) = jwt_public_key_pem {
            ForwardVerifier::Pem { pem }
        } else if let Some(url) = jwks_url {
            ForwardVerifier::Jwks(JwksVerifier {
                url,
                client: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_millis(timeout_ms))
                    .build()
                    .map_err(AppError::internal)?,
                cache: Arc::new(RwLock::new(None)),
            })
        } else {
            return Err(AppError::config(
                "remote auth requires a JWT verification method",
            ));
        };

        Ok(Self {
            username_claim,
            groups_claim,
            issuer,
            audience,
            verifier,
        })
    }

    pub async fn validate_identity(&self, token: &str) -> Result<AuthenticatedIdentity, AppError> {
        let claims = match &self.verifier {
            ForwardVerifier::Hmac { secret } => {
                let mut validation = Validation::new(Algorithm::HS256);
                validation.algorithms = vec![Algorithm::HS256, Algorithm::HS384, Algorithm::HS512];
                apply_issuer_audience_validation(
                    &mut validation,
                    &self.issuer,
                    self.audience.as_deref(),
                );
                decode::<Value>(
                    token,
                    &DecodingKey::from_secret(secret.as_bytes()),
                    &validation,
                )
                .map_err(jwt_error_to_unauthorized)?
                .claims
            }
            ForwardVerifier::Pem { pem } => {
                let (key, algorithms) = if pem.contains("BEGIN EC") {
                    (
                        DecodingKey::from_ec_pem(pem.as_bytes()).map_err(AppError::internal)?,
                        vec![Algorithm::ES256, Algorithm::ES384],
                    )
                } else {
                    (
                        DecodingKey::from_rsa_pem(pem.as_bytes()).map_err(AppError::internal)?,
                        vec![Algorithm::RS256, Algorithm::RS384, Algorithm::RS512],
                    )
                };
                let mut validation = Validation::new(algorithms[0]);
                validation.algorithms = algorithms;
                apply_issuer_audience_validation(
                    &mut validation,
                    &self.issuer,
                    self.audience.as_deref(),
                );
                decode::<Value>(token, &key, &validation)
                    .map_err(jwt_error_to_unauthorized)?
                    .claims
            }
            ForwardVerifier::Jwks(jwks) => {
                let header = decode_header(token).map_err(jwt_error_to_unauthorized)?;
                let kid = header
                    .kid
                    .ok_or_else(|| AppError::unauthorized("remote token is missing `kid`"))?;
                let jwk = jwks.find_key(&kid).await?;
                let mut validation = Validation::new(header.alg);
                validation.algorithms = vec![
                    Algorithm::RS256,
                    Algorithm::RS384,
                    Algorithm::RS512,
                    Algorithm::ES256,
                    Algorithm::ES384,
                ];
                apply_issuer_audience_validation(
                    &mut validation,
                    &self.issuer,
                    self.audience.as_deref(),
                );
                let key = DecodingKey::from_jwk(&jwk).map_err(AppError::internal)?;
                decode::<Value>(token, &key, &validation)
                    .map_err(jwt_error_to_unauthorized)?
                    .claims
            }
        };
        claims_to_authenticated_identity(&claims, &self.username_claim, &self.groups_claim)
    }
}

impl JwksVerifier {
    async fn find_key(&self, kid: &str) -> Result<jsonwebtoken::jwk::Jwk, AppError> {
        // Try cache first (if not stale)
        if let Some(jwk) = self.lookup_cached(kid).await {
            return Ok(jwk);
        }

        // Fetch fresh keys
        let set = self.fetch_jwks().await?;
        set.keys
            .iter()
            .find(|jwk| jwk.common.key_id.as_deref() == Some(kid))
            .cloned()
            .ok_or_else(|| AppError::unauthorized(format!("no JWKS key found for kid `{kid}`")))
    }

    async fn fetch_jwks(&self) -> Result<JwkSet, AppError> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|error| AppError::unavailable(format!("failed to fetch JWKS: {error}")))?;
        if !response.status().is_success() {
            return Err(AppError::unavailable(format!(
                "JWKS endpoint returned {}",
                response.status()
            )));
        }
        let set = response
            .json::<JwkSet>()
            .await
            .map_err(AppError::internal)?;
        let mut cache = self.cache.write().await;
        *cache = Some(JwksCacheEntry {
            set: set.clone(),
            fetched_at: Utc::now(),
        });
        Ok(set)
    }

    async fn lookup_cached(&self, kid: &str) -> Option<jsonwebtoken::jwk::Jwk> {
        let cache = self.cache.read().await;
        cache.as_ref().and_then(|entry| {
            // Cache is stale — treat as miss
            let age = Utc::now() - entry.fetched_at;
            if age.num_seconds() > JWKS_CACHE_TTL_SECS {
                return None;
            }
            entry
                .set
                .keys
                .iter()
                .find(|jwk| jwk.common.key_id.as_deref() == Some(kid))
                .cloned()
        })
    }
}

fn apply_issuer_audience_validation(
    validation: &mut Validation,
    issuer: &str,
    audience: Option<&str>,
) {
    validation.set_issuer(&[issuer]);
    if let Some(audience) = audience {
        validation.set_audience(&[audience]);
    }
}

fn claims_to_authenticated_identity(
    claims: &Value,
    username_claim: &str,
    groups_claim: &str,
) -> Result<AuthenticatedIdentity, AppError> {
    let username = claims
        .get(username_claim)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::unauthorized(format!(
                "token is missing username claim `{username_claim}`"
            ))
        })?;
    let exp = claims
        .get("exp")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::unauthorized("token is missing `exp`"))?;
    let expires_at = Utc
        .timestamp_opt(exp, 0)
        .single()
        .ok_or_else(|| AppError::unauthorized("invalid token expiry"))?;
    let groups = match claims.get(groups_claim) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::Null) | None => Vec::new(),
        _ => {
            return Err(AppError::unauthorized(format!(
                "token claim `{groups_claim}` must be an array"
            )));
        }
    };
    Ok(AuthenticatedIdentity {
        username: username.to_string(),
        groups,
        max_expires_at: Some(expires_at),
    })
}

fn jwt_error_to_unauthorized(error: jsonwebtoken::errors::Error) -> AppError {
    AppError::unauthorized(format!("invalid bearer token: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_token_round_trip() {
        let principal = Principal {
            id: "alice".to_string(),
            namespace: vec!["mreg".to_string(), "local".to_string()],
            groups: vec![Group {
                id: "ops".to_string(),
                namespace: vec!["mreg".to_string(), "local".to_string()],
            }],
        };
        let issuer = LocalJwtIssuer::new("secret", "mreg-rust", 300);
        let (token, _) = issuer
            .issue_access_token(&principal, "alice", "local", "local", None)
            .expect("issue token");
        let validator = LocalJwtValidator::new("secret", "mreg-rust");
        let context = validator.validate(&token).expect("validate token");
        assert_eq!(context.principal.id, "alice");
        assert_eq!(
            context.principal.namespace,
            vec!["mreg".to_string(), "local".to_string()]
        );
        assert_eq!(context.principal.key(), "mreg::local::alice");
        assert_eq!(context.username, "alice");
        assert_eq!(context.auth_scope.as_deref(), Some("local"));
        assert_eq!(context.auth_provider_kind.as_deref(), Some("local"));
        assert_eq!(context.principal.groups.len(), 1);
        assert_eq!(context.principal.groups[0].id, "ops");
        assert_eq!(context.principal.groups[0].key(), "mreg::local::ops");
    }

    #[test]
    fn local_token_rejects_wrong_secret() {
        let principal = Principal {
            id: "alice".to_string(),
            namespace: vec!["mreg".to_string(), "local".to_string()],
            groups: Vec::new(),
        };
        let issuer = LocalJwtIssuer::new("secret", "mreg-rust", 300);
        let (token, _) = issuer
            .issue_access_token(&principal, "alice", "local", "local", None)
            .expect("issue token");
        let validator = LocalJwtValidator::new("wrong", "mreg-rust");
        assert!(validator.validate(&token).is_err());
    }

    #[test]
    fn local_token_rejects_wrong_issuer() {
        let principal = Principal {
            id: "alice".to_string(),
            namespace: vec!["mreg".to_string(), "local".to_string()],
            groups: Vec::new(),
        };
        let issuer = LocalJwtIssuer::new("secret", "mreg-rust", 300);
        let (token, _) = issuer
            .issue_access_token(&principal, "alice", "local", "local", None)
            .expect("issue token");
        let validator = LocalJwtValidator::new("secret", "wrong-issuer");
        let error = validator
            .validate(&token)
            .expect_err("should reject wrong issuer");
        assert!(error.to_string().contains("bearer token"), "error: {error}");
    }

    #[test]
    fn local_token_rejects_expired() {
        let principal = Principal {
            id: "alice".to_string(),
            namespace: vec!["mreg".to_string(), "local".to_string()],
            groups: Vec::new(),
        };
        // Issue a valid token, then force expiry by setting max_expires_at in the past
        let issuer = LocalJwtIssuer::new("secret", "mreg-rust", 300);
        let past = Utc::now() - Duration::seconds(120);
        let (token, _) = issuer
            .issue_access_token(&principal, "alice", "local", "local", Some(past))
            .expect("issue token");
        let validator = LocalJwtValidator::new("secret", "mreg-rust");
        let error = validator
            .validate(&token)
            .expect_err("should reject expired token");
        assert!(error.to_string().contains("bearer token"), "error: {error}");
    }

    #[test]
    fn local_token_rejects_malformed() {
        let validator = LocalJwtValidator::new("secret", "mreg-rust");
        assert!(validator.validate("not-a-jwt").is_err());
        assert!(validator.validate("").is_err());
        assert!(validator.validate("a.b.c").is_err());
    }

    #[test]
    fn login_request_debug_redacts_password() {
        let req = super::super::LoginRequest {
            username: "alice".to_string(),
            password: "s3cret".to_string(),
            service_name: None,
            otp_code: Some("123456".to_string()),
        };
        let debug = format!("{req:?}");
        assert!(
            !debug.contains("s3cret"),
            "password must be redacted: {debug}"
        );
        assert!(
            debug.contains("[REDACTED]"),
            "should show [REDACTED]: {debug}"
        );
        assert!(
            debug.contains("alice"),
            "username should be visible: {debug}"
        );
    }
}
