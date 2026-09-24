use async_trait::async_trait;
use uuid::Uuid;

use crate::{audit::OutboxClaim, errors::AppError};

/// Durable delivery state for audit-backed domain events.
#[async_trait]
pub trait OutboxStore: Send + Sync {
    async fn claim_events(&self, limit: u32) -> Result<Vec<OutboxClaim>, AppError>;
    async fn complete_event(&self, event_id: Uuid, lease_id: Uuid) -> Result<(), AppError>;
    async fn retry_event(
        &self,
        event_id: Uuid,
        lease_id: Uuid,
        error: &str,
        delay_seconds: u32,
    ) -> Result<(), AppError>;
}
