use serde_json::json;

use crate::{
    audit::{CreateHistoryEvent, actions, actor},
    domain::{
        filters::HostContactFilter,
        host_contact::{CreateHostContact, HostContact},
        pagination::{Page, PageRequest},
        types::EmailAddressValue,
    },
    errors::AppError,
    events::{DomainEvent, EventSinkClient},
    storage::{DynStorage, HostContactStore},
};

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_contact"))]
pub async fn list_host_contacts(
    store: &(dyn HostContactStore + Send + Sync),
    page: &PageRequest,
    filter: &HostContactFilter,
) -> Result<Page<HostContact>, AppError> {
    store.list_host_contacts(page, filter).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_contact"))]
pub async fn create_host_contact(
    storage: &DynStorage,
    command: CreateHostContact,
    events: &EventSinkClient,
) -> Result<HostContact, AppError> {
    let (contact, history) = storage
        .transaction(move |tx| {
            let contact = tx.host_contacts().create_host_contact(command)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_contact",
                Some(contact.id()),
                contact.email().as_str(),
                actions::CREATE,
                json!({"email": contact.email().as_str(), "display_name": contact.display_name()}),
            ))?;
            Ok((contact, event))
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(contact)
}

#[tracing::instrument(level = "debug", skip(store), fields(resource_kind = "host_contact"))]
pub async fn get_host_contact(
    store: &(dyn HostContactStore + Send + Sync),
    email: &EmailAddressValue,
) -> Result<HostContact, AppError> {
    store.get_host_contact_by_email(email).await
}

#[tracing::instrument(skip(storage, events), fields(resource_kind = "host_contact"))]
pub async fn delete_host_contact(
    storage: &DynStorage,
    email: &EmailAddressValue,
    events: &EventSinkClient,
) -> Result<(), AppError> {
    let email_owned = email.clone();
    let history = storage
        .transaction(move |tx| {
            let old = tx.host_contacts().get_host_contact_by_email(&email_owned)?;
            tx.host_contacts().delete_host_contact(&email_owned)?;
            let event = tx.audit().record_event(CreateHistoryEvent::new(
                actor::SYSTEM,
                "host_contact",
                Some(old.id()),
                old.email().as_str(),
                actions::DELETE,
                json!({"email": old.email().as_str(), "display_name": old.display_name()}),
            ))?;
            Ok(event)
        })
        .await?;

    events.emit(&DomainEvent::from(&history)).await;

    Ok(())
}
