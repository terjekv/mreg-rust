use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{Page, PageRequest},
        types::DnsName,
    },
    errors::AppError,
    storage::NameServerStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

pub(super) fn create_nameserver_in_state(
    state: &mut MemoryState,
    command: CreateNameServer,
) -> Result<NameServer, AppError> {
    let key = command.name().as_str().to_string();
    if state.nameservers.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "nameserver '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let nameserver = NameServer::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.ttl(),
        now,
        now,
    )?;
    state.nameservers.insert(key, nameserver.clone());
    Ok(nameserver)
}

#[async_trait]
impl NameServerStore for MemoryStorage {
    async fn list_nameservers(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<NameServer> = state.nameservers.values().cloned().collect();
        sort_items(&mut items, page, |ns, field| match field {
            "created_at" => ns.created_at().to_rfc3339(),
            _ => ns.name().as_str().to_string(),
        });
        paginate_by_cursor(items, page)
    }

    async fn create_nameserver(&self, command: CreateNameServer) -> Result<NameServer, AppError> {
        let mut state = self.state.write().await;
        let ns = create_nameserver_in_state(&mut state, command)?;
        Ok(ns)
    }

    async fn get_nameserver_by_name(&self, name: &DnsName) -> Result<NameServer, AppError> {
        let state = self.state.read().await;
        state
            .nameservers
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("nameserver '{}' was not found", name.as_str()))
            })
    }

    async fn update_nameserver(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        let mut state = self.state.write().await;
        let ns = state
            .nameservers
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!("nameserver '{}' was not found", name.as_str()))
            })?;
        let now = Utc::now();
        let ttl = match command.ttl {
            Some(new_ttl) => new_ttl,
            None => ns.ttl(),
        };
        let updated = NameServer::restore(ns.id(), ns.name().clone(), ttl, ns.created_at(), now)?;
        state
            .nameservers
            .insert(name.as_str().to_string(), updated.clone());
        Ok(updated)
    }

    async fn delete_nameserver(&self, name: &DnsName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        if state.forward_zones.values().any(|zone| {
            zone.nameservers()
                .iter()
                .any(|nameserver| nameserver == name)
        }) || state.reverse_zones.values().any(|zone| {
            zone.nameservers()
                .iter()
                .any(|nameserver| nameserver == name)
        }) {
            return Err(AppError::conflict(
                "nameserver is still referenced by another resource",
            ));
        }

        match state.nameservers.remove(name.as_str()) {
            Some(_removed) => Ok(()),
            None => Err(AppError::not_found(format!(
                "nameserver '{}' was not found",
                name.as_str()
            ))),
        }
    }
}
