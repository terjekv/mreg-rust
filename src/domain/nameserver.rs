use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{DnsName, Ttl, UpdateField},
    errors::AppError,
};

/// Registered nameserver with an optional TTL override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameServer {
    id: Uuid,
    name: DnsName,
    ttl: Option<Ttl>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NameServer {
    pub fn restore(
        id: Uuid,
        name: DnsName,
        ttl: Option<Ttl>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            id,
            name,
            ttl,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &DnsName {
        &self.name
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to register a new nameserver.
#[derive(Clone, Debug)]
pub struct CreateNameServer {
    name: DnsName,
    ttl: Option<Ttl>,
}

impl CreateNameServer {
    pub fn new(name: DnsName, ttl: Option<Ttl>) -> Self {
        Self { name, ttl }
    }

    pub fn name(&self) -> &DnsName {
        &self.name
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }
}

/// Partial update for a nameserver.
#[derive(Clone, Debug)]
pub struct UpdateNameServer {
    pub ttl: UpdateField<Ttl>,
}
