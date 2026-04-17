use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{errors::AppError, storage::AuthSessionStore};

use super::MemoryStorage;

#[async_trait]
impl AuthSessionStore for MemoryStorage {
    async fn revoke_token(
        &self,
        token_fingerprint: String,
        principal_id: String,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .revoked_tokens
            .insert(token_fingerprint, (principal_id, expires_at));
        Ok(())
    }

    async fn is_token_revoked(&self, token_fingerprint: &str) -> Result<bool, AppError> {
        let mut state = self.state.write().await;
        let now = Utc::now();
        state
            .revoked_tokens
            .retain(|_, (_, expires_at)| *expires_at > now);
        Ok(state.revoked_tokens.contains_key(token_fingerprint))
    }

    async fn revoke_all_for_principal(
        &self,
        principal_id: String,
        revoked_before: DateTime<Utc>,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        match state.principal_revoked_before.get_mut(&principal_id) {
            Some(existing) if *existing >= revoked_before => {}
            Some(existing) => *existing = revoked_before,
            None => {
                state
                    .principal_revoked_before
                    .insert(principal_id, revoked_before);
            }
        }
        Ok(())
    }

    async fn principal_revoked_before(
        &self,
        principal_id: &str,
    ) -> Result<Option<DateTime<Utc>>, AppError> {
        let state = self.state.read().await;
        Ok(state.principal_revoked_before.get(principal_id).copied())
    }
}
