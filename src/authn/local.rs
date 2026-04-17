use std::collections::HashMap;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use async_trait::async_trait;

use crate::{config::LocalUserConfig, errors::AppError};

use super::{AuthenticatedIdentity, BackendLoginRequest, ScopeAuthenticator};

#[derive(Clone)]
pub struct LocalScopeAuthenticator {
    users: HashMap<String, LocalUserConfig>,
}

impl LocalScopeAuthenticator {
    pub fn new(users: Vec<LocalUserConfig>) -> Self {
        Self {
            users: users
                .into_iter()
                .map(|user| (user.username.clone(), user))
                .collect(),
        }
    }
}

#[async_trait]
impl ScopeAuthenticator for LocalScopeAuthenticator {
    async fn login(
        &self,
        credentials: BackendLoginRequest,
    ) -> Result<AuthenticatedIdentity, AppError> {
        let user = self
            .users
            .get(&credentials.username)
            .ok_or_else(|| AppError::unauthorized("invalid credentials"))?;
        let parsed_hash = PasswordHash::new(&user.password_hash).map_err(|error| {
            AppError::unavailable(format!(
                "local auth password hash for `{}` is invalid: {error}",
                user.username
            ))
        })?;
        Argon2::default()
            .verify_password(credentials.password.as_bytes(), &parsed_hash)
            .map_err(|_| AppError::unauthorized("invalid credentials"))?;

        Ok(AuthenticatedIdentity {
            username: user.username.clone(),
            groups: user.groups.clone(),
            max_expires_at: None,
        })
    }
}
