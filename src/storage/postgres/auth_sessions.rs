use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, delete, insert_into, update,
};

use crate::{
    db::schema::{principal_token_revocations, revoked_tokens},
    errors::AppError,
    storage::AuthSessionStore,
};

use super::PostgresStorage;

#[async_trait]
impl AuthSessionStore for PostgresStorage {
    async fn revoke_token(
        &self,
        token_fingerprint: String,
        principal_id: String,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                insert_into(revoked_tokens::table)
                    .values((
                        revoked_tokens::token_fingerprint.eq(&token_fingerprint),
                        revoked_tokens::principal_id.eq(&principal_id),
                        revoked_tokens::expires_at.eq(expires_at),
                    ))
                    .on_conflict(revoked_tokens::token_fingerprint)
                    .do_update()
                    .set((
                        revoked_tokens::principal_id.eq(&principal_id),
                        revoked_tokens::expires_at.eq(expires_at),
                        revoked_tokens::revoked_at.eq(diesel::dsl::now),
                    ))
                    .execute(connection)?;
                Ok(())
            })
            .await
    }

    async fn is_token_revoked(&self, token_fingerprint: &str) -> Result<bool, AppError> {
        let token_fingerprint = token_fingerprint.to_string();
        self.database
            .run(move |connection| {
                let exists = revoked_tokens::table
                    .filter(revoked_tokens::token_fingerprint.eq(&token_fingerprint))
                    .filter(revoked_tokens::expires_at.gt(diesel::dsl::now))
                    .select(revoked_tokens::token_fingerprint)
                    .first::<String>(connection)
                    .optional()?
                    .is_some();
                Ok(exists)
            })
            .await
    }

    async fn revoke_all_for_principal(
        &self,
        principal_id: String,
        revoked_before: DateTime<Utc>,
    ) -> Result<(), AppError> {
        // PostgreSQL TIMESTAMPTZ has microsecond precision. Truncate nanoseconds before
        // storing so that a read-back of the stored value equals the value we wrote.
        let revoked_before = DateTime::from_timestamp_micros(revoked_before.timestamp_micros())
            .unwrap_or(revoked_before);
        self.database
            .run(move |connection| {
                let existing = principal_token_revocations::table
                    .filter(principal_token_revocations::principal_id.eq(&principal_id))
                    .select(principal_token_revocations::revoked_before)
                    .first::<DateTime<Utc>>(connection)
                    .optional()?;
                match existing {
                    Some(current) if current >= revoked_before => Ok(()),
                    Some(_) => {
                        update(
                            principal_token_revocations::table.filter(
                                principal_token_revocations::principal_id.eq(&principal_id),
                            ),
                        )
                        .set((
                            principal_token_revocations::revoked_before.eq(revoked_before),
                            principal_token_revocations::updated_at.eq(diesel::dsl::now),
                        ))
                        .execute(connection)?;
                        Ok(())
                    }
                    None => {
                        insert_into(principal_token_revocations::table)
                            .values((
                                principal_token_revocations::principal_id.eq(&principal_id),
                                principal_token_revocations::revoked_before.eq(revoked_before),
                            ))
                            .execute(connection)?;
                        Ok(())
                    }
                }
            })
            .await
    }

    async fn principal_revoked_before(
        &self,
        principal_id: &str,
    ) -> Result<Option<DateTime<Utc>>, AppError> {
        let principal_id = principal_id.to_string();
        self.database
            .run(move |connection| {
                principal_token_revocations::table
                    .filter(principal_token_revocations::principal_id.eq(&principal_id))
                    .select(principal_token_revocations::revoked_before)
                    .first::<DateTime<Utc>>(connection)
                    .optional()
                    .map_err(AppError::from)
            })
            .await
    }

    async fn prune_expired_tokens(&self) -> Result<u64, AppError> {
        self.database
            .run(|connection| {
                let count = delete(
                    revoked_tokens::table.filter(revoked_tokens::expires_at.lt(diesel::dsl::now)),
                )
                .execute(connection)?;
                Ok(count as u64)
            })
            .await
    }
}
