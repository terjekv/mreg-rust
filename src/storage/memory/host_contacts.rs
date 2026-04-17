use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::HostContactFilter,
        host_contact::{CreateHostContact, HostContact},
        pagination::{Page, PageRequest},
        types::EmailAddressValue,
    },
    errors::AppError,
    storage::HostContactStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

pub(super) fn create_host_contact_in_state(
    state: &mut MemoryState,
    command: CreateHostContact,
) -> Result<HostContact, AppError> {
    let key = command.email().as_str().to_string();
    if state.host_contacts.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "host contact '{}' already exists",
            key
        )));
    }
    for host in command.hosts() {
        if !state.hosts.contains_key(host.as_str()) {
            return Err(AppError::not_found(format!(
                "host '{}' was not found",
                host.as_str()
            )));
        }
    }
    let now = Utc::now();
    let contact = HostContact::restore(
        Uuid::new_v4(),
        command.email().clone(),
        command.display_name().map(str::to_string),
        command.hosts().to_vec(),
        now,
        now,
    )?;
    state.host_contacts.insert(key, contact.clone());
    Ok(contact)
}

#[async_trait]
impl HostContactStore for MemoryStorage {
    async fn list_host_contacts(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<HostContact> = state
            .host_contacts
            .values()
            .filter(|contact| filter.matches(contact))
            .cloned()
            .collect();
        sort_items(&mut items, page, |contact, field| match field {
            "display_name" => contact.display_name().unwrap_or("").to_string(),
            "created_at" => contact.created_at().to_rfc3339(),
            "updated_at" => contact.updated_at().to_rfc3339(),
            _ => contact.email().as_str().to_string(),
        });
        paginate_by_cursor(items, page)
    }

    async fn create_host_contact(
        &self,
        command: CreateHostContact,
    ) -> Result<HostContact, AppError> {
        let mut state = self.state.write().await;
        create_host_contact_in_state(&mut state, command)
    }

    async fn get_host_contact_by_email(
        &self,
        email: &EmailAddressValue,
    ) -> Result<HostContact, AppError> {
        let state = self.state.read().await;
        state
            .host_contacts
            .get(email.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("host contact '{}' was not found", email.as_str()))
            })
    }

    async fn delete_host_contact(&self, email: &EmailAddressValue) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        state
            .host_contacts
            .remove(email.as_str())
            .map(|_| ())
            .ok_or_else(|| {
                AppError::not_found(format!("host contact '{}' was not found", email.as_str()))
            })
    }
}
