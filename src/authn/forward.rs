use async_trait::async_trait;
use serde::Deserialize;

use crate::errors::AppError;

use super::jwt::{ForwardJwtValidator, ForwardJwtValidatorConfig};
use super::{AuthenticatedIdentity, BackendLoginRequest, ScopeAuthenticator};

pub struct RemoteScopeConfig {
    pub login_url: String,
    pub timeout_ms: u64,
    pub default_service_name: Option<String>,
    pub issuer: String,
    pub audience: Option<String>,
    pub jwks_url: Option<String>,
    pub jwt_public_key_pem: Option<String>,
    pub jwt_hmac_secret: Option<String>,
    pub username_claim: String,
    pub groups_claim: String,
}

#[derive(Clone)]
pub struct RemoteScopeAuthenticator {
    client: reqwest::Client,
    login_url: String,
    default_service_name: Option<String>,
    validator: ForwardJwtValidator,
}

#[derive(Deserialize)]
struct RemoteLoginResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

impl RemoteScopeAuthenticator {
    pub fn new(config: RemoteScopeConfig) -> Result<Self, AppError> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(config.timeout_ms))
                .build()
                .map_err(AppError::internal)?,
            login_url: config.login_url,
            default_service_name: config.default_service_name,
            validator: ForwardJwtValidator::new(ForwardJwtValidatorConfig {
                issuer: config.issuer,
                audience: config.audience,
                jwks_url: config.jwks_url,
                jwt_public_key_pem: config.jwt_public_key_pem,
                jwt_hmac_secret: config.jwt_hmac_secret,
                username_claim: config.username_claim,
                groups_claim: config.groups_claim,
                timeout_ms: config.timeout_ms,
            })?,
        })
    }
}

#[async_trait]
impl ScopeAuthenticator for RemoteScopeAuthenticator {
    async fn login(
        &self,
        mut credentials: BackendLoginRequest,
    ) -> Result<AuthenticatedIdentity, AppError> {
        if credentials.service_name.is_none() {
            credentials.service_name = self.default_service_name.clone();
        }
        let response = self
            .client
            .post(&self.login_url)
            .json(&credentials)
            .send()
            .await
            .map_err(|error| {
                AppError::unavailable(format!("remote auth request failed: {error}"))
            })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(AppError::unauthorized("invalid credentials"));
        }
        if !response.status().is_success() {
            return Err(AppError::unavailable(format!(
                "remote auth endpoint returned {}",
                response.status()
            )));
        }
        let payload = response
            .json::<RemoteLoginResponse>()
            .await
            .map_err(AppError::internal)?;
        let access_token = payload
            .access_token
            .or(payload.token)
            .ok_or_else(|| AppError::unavailable("remote auth response is missing access_token"))?;
        self.validator.validate_identity(&access_token).await
    }
}
