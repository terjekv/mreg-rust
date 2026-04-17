use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{EmailAddressValue, Hostname},
    errors::AppError,
};

/// Contact email associated with one or more hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostContact {
    id: Uuid,
    email: EmailAddressValue,
    display_name: Option<String>,
    hosts: Vec<Hostname>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostContact {
    pub fn restore(
        id: Uuid,
        email: EmailAddressValue,
        display_name: Option<String>,
        hosts: Vec<Hostname>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            id,
            email,
            display_name: normalize_optional_text(display_name),
            hosts: dedupe_hosts(hosts),
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn email(&self) -> &EmailAddressValue {
        &self.email
    }
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    pub fn hosts(&self) -> &[Hostname] {
        &self.hosts
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a host contact association.
#[derive(Clone, Debug)]
pub struct CreateHostContact {
    email: EmailAddressValue,
    display_name: Option<String>,
    hosts: Vec<Hostname>,
}

impl CreateHostContact {
    pub fn new(
        email: EmailAddressValue,
        display_name: Option<String>,
        hosts: Vec<Hostname>,
    ) -> Self {
        Self {
            email,
            display_name: normalize_optional_text(display_name),
            hosts: dedupe_hosts(hosts),
        }
    }

    pub fn email(&self) -> &EmailAddressValue {
        &self.email
    }
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
    pub fn hosts(&self) -> &[Hostname] {
        &self.hosts
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn dedupe_hosts(mut hosts: Vec<Hostname>) -> Vec<Hostname> {
    let mut seen = BTreeSet::new();
    hosts.retain(|host| seen.insert(host.clone()));
    hosts
}
