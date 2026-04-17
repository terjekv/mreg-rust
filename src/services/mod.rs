pub mod attachments;
pub mod bacnet;
pub mod communities;
pub mod exports;
pub mod host_community_assignments;
pub mod host_contacts;
pub mod host_groups;
pub mod host_policy;
pub mod hosts;
pub mod imports;
pub mod labels;
pub mod nameservers;
pub mod network_policies;
pub mod networks;
pub mod ptr_overrides;
pub mod records;
pub mod tasks;
pub mod zones;

use crate::{
    audit::CreateHistoryEvent,
    events::{DomainEvent, EventSinkClient},
    storage::AuditStore,
};

/// Record an audit event and emit a domain event. If the audit store fails,
/// logs a warning with the action, resource kind, and resource name so the
/// failure is diagnosable without reconstructing context from parent spans.
pub async fn record_and_emit(
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    event: CreateHistoryEvent,
) {
    let resource_kind = event.resource_kind().to_string();
    let resource_name = event.resource_name().to_string();
    let action = event.action().to_string();
    let domain_event = DomainEvent::from(&event);

    if let Err(error) = audit.record_event(event).await {
        tracing::warn!(
            %resource_kind,
            %resource_name,
            %action,
            %error,
            "failed to record audit event"
        );
    }

    events.emit(&domain_event).await;
}
