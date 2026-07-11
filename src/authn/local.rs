use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    thread::available_parallelism,
};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use async_trait::async_trait;
use tokio::{sync::Semaphore, task::spawn_blocking};

use crate::{config::LocalUserConfig, errors::AppError};

use super::{AuthenticatedIdentity, BackendLoginRequest, ScopeAuthenticator};

#[derive(Clone)]
pub struct LocalScopeAuthenticator {
    users: HashMap<String, LocalUserConfig>,
    dummy_hash: String,
}

impl LocalScopeAuthenticator {
    pub fn new(users: Vec<LocalUserConfig>) -> Self {
        let salt = SaltString::encode_b64(b"mreg-dummy-salt").expect("static dummy salt is valid");
        let dummy_hash = Argon2::default()
            .hash_password(b"invalid-password", &salt)
            .expect("static dummy password can be hashed")
            .to_string();
        Self {
            users: users
                .into_iter()
                .map(|user| (user.username.clone(), user))
                .collect(),
            dummy_hash,
        }
    }
}

fn verification_limiter() -> Arc<Semaphore> {
    static LIMITER: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(LIMITER.get_or_init(|| {
        let parallelism = available_parallelism()
            .map(usize::from)
            .unwrap_or(2)
            .clamp(2, 8);
        Arc::new(Semaphore::new(parallelism))
    }))
}

#[async_trait]
impl ScopeAuthenticator for LocalScopeAuthenticator {
    async fn login(
        &self,
        credentials: BackendLoginRequest,
    ) -> Result<AuthenticatedIdentity, AppError> {
        let user = self.users.get(&credentials.username).cloned();
        let password_hash = user
            .as_ref()
            .map(|user| user.password_hash.clone())
            .unwrap_or_else(|| self.dummy_hash.clone());
        let password = credentials.password;
        let _permit = verification_limiter()
            .acquire_owned()
            .await
            .map_err(|_| AppError::unavailable("password verifier is shutting down"))?;
        let verification = spawn_blocking(move || {
            let parsed_hash =
                PasswordHash::new(&password_hash).map_err(|error| error.to_string())?;
            Ok::<bool, String>(
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed_hash)
                    .is_ok(),
            )
        })
        .await
        .map_err(AppError::internal)?;

        let verified = verification.map_err(|error| {
            AppError::unavailable(format!(
                "configured local password hash is invalid: {error}"
            ))
        })?;
        let user = user
            .filter(|_| verified)
            .ok_or_else(|| AppError::unauthorized("invalid credentials"))?;

        Ok(AuthenticatedIdentity {
            username: user.username,
            groups: user.groups,
            max_expires_at: None,
        })
    }
}
