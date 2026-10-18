use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    thread::available_parallelism,
};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use async_trait::async_trait;
use tokio::{sync::Semaphore, task::spawn_blocking};

use crate::{config::LocalUserConfig, errors::AppError};

use super::{AuthProviderBackend, AuthenticatedIdentity, BackendLoginRequest};

#[derive(Clone)]
pub struct LocalAuthProvider {
    users: HashMap<String, LocalUserConfig>,
    dummy_hash: String,
}

impl LocalAuthProvider {
    pub fn new(users: Vec<LocalUserConfig>) -> Self {
        let dummy_hash = Argon2::default()
            .hash_password_with_salt(b"invalid-password", b"mreg-dummy-salt")
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
impl AuthProviderBackend for LocalAuthProvider {
    async fn authenticate(
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("password", true)]
    #[case("wrong-password", false)]
    #[tokio::test]
    async fn verifies_existing_argon2_phc_hashes(#[case] password: &str, #[case] accepted: bool) {
        // Fixed Argon2id v19 reference vector also supported by argon2 0.5.
        let provider = LocalAuthProvider::new(vec![LocalUserConfig {
            username: "admin".to_string(),
            password_hash: "$argon2id$v=19$m=256,t=2,p=1$c29tZXNhbHQ$nf65EOgLrQMR/uIPnA4rEsF5h7TKyQwu9U1bMCHGi/4".to_string(),
            groups: Vec::new(),
        }]);
        let result = provider
            .authenticate(BackendLoginRequest {
                username: "admin".to_string(),
                password: password.to_string(),
                service_name: None,
                otp_code: None,
            })
            .await;

        assert_eq!(result.is_ok(), accepted);
    }
}
