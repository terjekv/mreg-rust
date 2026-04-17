use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{HostGroupName, Hostname, OwnerGroupName},
    errors::AppError,
};

/// Named group of hosts with parent groups and owner groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGroup {
    id: Uuid,
    name: HostGroupName,
    description: String,
    hosts: Vec<Hostname>,
    parent_groups: Vec<HostGroupName>,
    owner_groups: Vec<OwnerGroupName>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostGroup {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        name: HostGroupName,
        description: impl Into<String>,
        hosts: Vec<Hostname>,
        parent_groups: Vec<HostGroupName>,
        owner_groups: Vec<OwnerGroupName>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let description = normalize_required_text(description.into(), "host group description")?;
        Ok(Self {
            id,
            name,
            description,
            hosts: dedupe_hosts(hosts),
            parent_groups: dedupe_names(parent_groups),
            owner_groups: dedupe_names(owner_groups),
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn name(&self) -> &HostGroupName {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn hosts(&self) -> &[Hostname] {
        &self.hosts
    }
    pub fn parent_groups(&self) -> &[HostGroupName] {
        &self.parent_groups
    }
    pub fn owner_groups(&self) -> &[OwnerGroupName] {
        &self.owner_groups
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new host group.
#[derive(Clone, Debug)]
pub struct CreateHostGroup {
    name: HostGroupName,
    description: String,
    hosts: Vec<Hostname>,
    parent_groups: Vec<HostGroupName>,
    owner_groups: Vec<OwnerGroupName>,
}

impl CreateHostGroup {
    pub fn new(
        name: HostGroupName,
        description: impl Into<String>,
        hosts: Vec<Hostname>,
        parent_groups: Vec<HostGroupName>,
        owner_groups: Vec<OwnerGroupName>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            name,
            description: normalize_required_text(description.into(), "host group description")?,
            hosts: dedupe_hosts(hosts),
            parent_groups: dedupe_names(parent_groups),
            owner_groups: dedupe_names(owner_groups),
        })
    }

    pub fn name(&self) -> &HostGroupName {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn hosts(&self) -> &[Hostname] {
        &self.hosts
    }
    pub fn parent_groups(&self) -> &[HostGroupName] {
        &self.parent_groups
    }
    pub fn owner_groups(&self) -> &[OwnerGroupName] {
        &self.owner_groups
    }
}

fn normalize_required_text(value: String, label: &str) -> Result<String, AppError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::validation(format!("{label} cannot be empty")));
    }
    Ok(trimmed)
}

fn dedupe_hosts(mut hosts: Vec<Hostname>) -> Vec<Hostname> {
    let mut seen = BTreeSet::new();
    hosts.retain(|host| seen.insert(host.clone()));
    hosts
}

fn dedupe_names<T>(mut items: Vec<T>) -> Vec<T>
where
    T: Ord + Clone,
{
    let mut seen = BTreeSet::new();
    items.retain(|item| seen.insert(item.clone()));
    items
}
