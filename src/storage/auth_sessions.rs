use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::errors::AppError;

#[async_trait]
pub trait AuthSessionStore: Send + Sync {
    async fn revoke_token(
        &self,
        token_fingerprint: String,
        principal_id: String,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError>;

    async fn is_token_revoked(&self, token_fingerprint: &str) -> Result<bool, AppError>;

    async fn revoke_all_for_principal(
        &self,
        principal_id: String,
        revoked_before: DateTime<Utc>,
    ) -> Result<(), AppError>;

    async fn principal_revoked_before(
        &self,
        principal_id: &str,
    ) -> Result<Option<DateTime<Utc>>, AppError>;

    /// Delete revoked_tokens rows whose expires_at is in the past.
    /// Returns the number of rows deleted. Safe to call concurrently.
    async fn prune_expired_tokens(&self) -> Result<u64, AppError>;
}
