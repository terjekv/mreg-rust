use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::types::HostPolicyName;

/// A single policy atom that can be assigned to roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPolicyAtom {
    id: Uuid,
    name: HostPolicyName,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostPolicyAtom {
    /// Reconstruct an atom from persisted data.
    pub fn restore(
        id: Uuid,
        name: HostPolicyName,
        description: String,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            description,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &HostPolicyName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new host-policy atom.
#[derive(Clone, Debug)]
pub struct CreateHostPolicyAtom {
    name: HostPolicyName,
    description: String,
}

impl CreateHostPolicyAtom {
    pub fn new(name: HostPolicyName, description: impl Into<String>) -> Self {
        Self {
            name,
            description: description.into(),
        }
    }

    pub fn name(&self) -> &HostPolicyName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Partial update for an atom's description.
#[derive(Clone, Debug)]
pub struct UpdateHostPolicyAtom {
    pub description: Option<String>,
}

/// A policy role that groups atoms and is assigned to hosts/labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPolicyRole {
    id: Uuid,
    name: HostPolicyName,
    description: String,
    atoms: Vec<String>,
    hosts: Vec<String>,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostPolicyRole {
    /// Reconstruct a role from persisted data.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        name: HostPolicyName,
        description: String,
        atoms: Vec<String>,
        hosts: Vec<String>,
        labels: Vec<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            description,
            atoms,
            hosts,
            labels,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &HostPolicyName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn atoms(&self) -> &[String] {
        &self.atoms
    }

    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new host-policy role.
#[derive(Clone, Debug)]
pub struct CreateHostPolicyRole {
    name: HostPolicyName,
    description: String,
}

impl CreateHostPolicyRole {
    pub fn new(name: HostPolicyName, description: impl Into<String>) -> Self {
        Self {
            name,
            description: description.into(),
        }
    }

    pub fn name(&self) -> &HostPolicyName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Partial update for a role's description.
#[derive(Clone, Debug)]
pub struct UpdateHostPolicyRole {
    pub description: Option<String>,
}
