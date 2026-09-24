use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    audit::{CreateHistoryEvent, HistoryEvent, OutboxClaim},
    domain::pagination::{Page, PageRequest},
    errors::AppError,
    storage::{AuditStore, OutboxStore},
};

use super::{MemoryEventDelivery, MemoryState, MemoryStorage, paginate_by_cursor};

pub(super) fn record_event_in_state(
    state: &mut MemoryState,
    event: CreateHistoryEvent,
) -> HistoryEvent {
    let now = Utc::now();
    let history_event = HistoryEvent::restore(
        Uuid::new_v4(),
        event.actor().to_string(),
        event.resource_kind().to_string(),
        event.resource_id(),
        event.resource_name().to_string(),
        event.action().to_string(),
        event.data().clone(),
        now,
    );
    state.history_events.push(history_event.clone());
    state.event_delivery.insert(
        history_event.id(),
        MemoryEventDelivery {
            attempts: 0,
            available_at: now,
            lease_id: None,
            lease_until: None,
            delivered_at: None,
            last_error: None,
        },
    );
    history_event
}

pub(super) fn list_events_in_state(
    state: &MemoryState,
    page: &PageRequest,
) -> Result<Page<HistoryEvent>, AppError> {
    let mut items: Vec<HistoryEvent> = state.history_events.clone();
    items.sort_by_key(|item| item.id());
    paginate_by_cursor(items, page)
}

#[async_trait]
impl AuditStore for MemoryStorage {
    async fn record_event(&self, event: CreateHistoryEvent) -> Result<HistoryEvent, AppError> {
        let mut state = self.state.write().await;
        Ok(record_event_in_state(&mut state, event))
    }

    async fn list_events(&self, page: &PageRequest) -> Result<Page<HistoryEvent>, AppError> {
        let state = self.state.read().await;
        list_events_in_state(&state, page)
    }
}

#[async_trait]
impl OutboxStore for MemoryStorage {
    async fn claim_events(&self, limit: u32) -> Result<Vec<OutboxClaim>, AppError> {
        let mut state = self.state.write().await;
        let now = Utc::now();
        let mut eligible = state
            .history_events
            .iter()
            .filter(|event| {
                state
                    .event_delivery
                    .get(&event.id())
                    .is_some_and(|delivery| {
                        delivery.delivered_at.is_none()
                            && delivery.available_at <= now
                            && delivery.lease_until.is_none_or(|until| until <= now)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        eligible.sort_by_key(HistoryEvent::created_at);
        eligible.truncate(limit as usize);

        let mut claims = Vec::with_capacity(eligible.len());
        for event in eligible {
            let lease_id = Uuid::new_v4();
            let delivery = state
                .event_delivery
                .get_mut(&event.id())
                .ok_or_else(|| AppError::internal("audit event has no delivery state"))?;
            delivery.attempts = delivery.attempts.saturating_add(1);
            delivery.lease_id = Some(lease_id);
            delivery.lease_until = Some(now + chrono::Duration::seconds(30));
            claims.push(OutboxClaim::new(event, lease_id, delivery.attempts));
        }
        Ok(claims)
    }

    async fn complete_event(&self, event_id: Uuid, lease_id: Uuid) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let delivery = state
            .event_delivery
            .get_mut(&event_id)
            .ok_or_else(|| AppError::not_found("event delivery state was not found"))?;
        if delivery.lease_id != Some(lease_id) {
            return Err(AppError::conflict("event delivery lease was lost"));
        }
        delivery.delivered_at = Some(Utc::now());
        delivery.lease_id = None;
        delivery.lease_until = None;
        delivery.last_error = None;
        Ok(())
    }

    async fn retry_event(
        &self,
        event_id: Uuid,
        lease_id: Uuid,
        error: &str,
        delay_seconds: u32,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let delivery = state
            .event_delivery
            .get_mut(&event_id)
            .ok_or_else(|| AppError::not_found("event delivery state was not found"))?;
        if delivery.lease_id != Some(lease_id) {
            return Err(AppError::conflict("event delivery lease was lost"));
        }
        delivery.available_at = Utc::now() + chrono::Duration::seconds(delay_seconds.into());
        delivery.lease_id = None;
        delivery.lease_until = None;
        delivery.last_error = Some(error.chars().take(2048).collect());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event() -> CreateHistoryEvent {
        CreateHistoryEvent::new("tester", "host", None, "host.example", "create", json!({}))
    }

    #[tokio::test]
    async fn audit_event_is_immediately_claimable() {
        let storage = MemoryStorage::new();
        let history = storage.record_event(event()).await.unwrap();
        let claims = storage.claim_events(1).await.unwrap();
        assert_eq!(claims[0].event().id(), history.id());
    }

    #[tokio::test]
    async fn completed_event_is_not_claimed_again() {
        let storage = MemoryStorage::new();
        storage.record_event(event()).await.unwrap();
        let claim = storage.claim_events(1).await.unwrap().remove(0);
        storage
            .complete_event(claim.event().id(), claim.lease_id())
            .await
            .unwrap();
        assert!(storage.claim_events(1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_event_is_reclaimed_after_retry_delay() {
        let storage = MemoryStorage::new();
        storage.record_event(event()).await.unwrap();
        let claim = storage.claim_events(1).await.unwrap().remove(0);
        storage
            .retry_event(claim.event().id(), claim.lease_id(), "failure", 0)
            .await
            .unwrap();
        let retried = storage.claim_events(1).await.unwrap().remove(0);
        assert_eq!(retried.attempt(), 2);
    }
}
