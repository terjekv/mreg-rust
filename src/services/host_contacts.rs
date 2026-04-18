use serde_json::json;

use crate::{
    audit::actions,
    domain::{
        filters::HostContactFilter,
        host_contact::{CreateHostContact, HostContact},
        pagination::{Page, PageRequest},
        types::EmailAddressValue,
    },
    errors::AppError,
    events::EventSinkClient,
    storage::{AuditStore, HostContactStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_contact"))]
pub async fn list_host_contacts(
    store: &(dyn HostContactStore + Send + Sync),
    page: &PageRequest,
    filter: &HostContactFilter,
) -> Result<Page<HostContact>, AppError> {
    store.list_host_contacts(page, filter).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_contact"))]
pub async fn create_host_contact(
    store: &(dyn HostContactStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    command: CreateHostContact,
) -> Result<HostContact, AppError> {
    let contact = store.create_host_contact(command).await?;

    super::audit_mutation(
        audit,
        events,
        "host_contact",
        actions::CREATE,
        Some(contact.id()),
        contact.email().as_str(),
        json!({"email": contact.email().as_str(), "display_name": contact.display_name()}),
    )
    .await;

    Ok(contact)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_contact"))]
pub async fn get_host_contact(
    store: &(dyn HostContactStore + Send + Sync),
    email: &EmailAddressValue,
) -> Result<HostContact, AppError> {
    store.get_host_contact_by_email(email).await
}

#[tracing::instrument(skip(store, audit, events), fields(resource_kind = "host_contact"))]
pub async fn delete_host_contact(
    store: &(dyn HostContactStore + Send + Sync),
    audit: &(dyn AuditStore + Send + Sync),
    events: &EventSinkClient,
    email: &EmailAddressValue,
) -> Result<(), AppError> {
    let old = store.get_host_contact_by_email(email).await?;
    store.delete_host_contact(email).await?;

    super::audit_mutation(
        audit,
        events,
        "host_contact",
        actions::DELETE,
        Some(old.id()),
        old.email().as_str(),
        json!({"email": old.email().as_str(), "display_name": old.display_name()}),
    )
    .await;

    Ok(())
}
