use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{NetworkPolicyAttributeName, NetworkPolicyName, UpdateField},
    errors::AppError,
};

/// Attribute definition that can be assigned a boolean value on a network policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPolicyAttribute {
    id: Uuid,
    name: NetworkPolicyAttributeName,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NetworkPolicyAttribute {
    pub fn restore(
        id: Uuid,
        name: NetworkPolicyAttributeName,
        description: impl Into<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            description: description.into(),
            created_at,
            updated_at,
        }
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn name(&self) -> &NetworkPolicyAttributeName {
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

#[derive(Clone, Debug)]
pub struct CreateNetworkPolicyAttribute {
    name: NetworkPolicyAttributeName,
    description: String,
}

impl CreateNetworkPolicyAttribute {
    pub fn new(name: NetworkPolicyAttributeName, description: impl Into<String>) -> Self {
        Self {
            name,
            description: description.into(),
        }
    }
    pub fn name(&self) -> &NetworkPolicyAttributeName {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
}

#[derive(Clone, Debug, Default)]
pub struct UpdateNetworkPolicyAttribute {
    pub name: Option<NetworkPolicyAttributeName>,
    pub description: Option<String>,
}

/// Resolved value of an attribute on a policy. `false` is a meaningful value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPolicyAttributeValue {
    attribute_id: Uuid,
    name: NetworkPolicyAttributeName,
    value: bool,
}

impl NetworkPolicyAttributeValue {
    pub fn restore(attribute_id: Uuid, name: NetworkPolicyAttributeName, value: bool) -> Self {
        Self {
            attribute_id,
            name,
            value,
        }
    }
    pub fn attribute_id(&self) -> Uuid {
        self.attribute_id
    }
    pub fn name(&self) -> &NetworkPolicyAttributeName {
        &self.name
    }
    pub fn value(&self) -> bool {
        self.value
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetNetworkPolicyAttributeValue {
    name: NetworkPolicyAttributeName,
    value: bool,
}

impl SetNetworkPolicyAttributeValue {
    pub fn new(name: NetworkPolicyAttributeName, value: bool) -> Self {
        Self { name, value }
    }
    pub fn name(&self) -> &NetworkPolicyAttributeName {
        &self.name
    }
    pub fn value(&self) -> bool {
        self.value
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPolicyDetails {
    policy: NetworkPolicy,
    attributes: Vec<NetworkPolicyAttributeValue>,
}

impl NetworkPolicyDetails {
    pub fn new(policy: NetworkPolicy, attributes: Vec<NetworkPolicyAttributeValue>) -> Self {
        Self { policy, attributes }
    }
    pub fn policy(&self) -> &NetworkPolicy {
        &self.policy
    }
    pub fn attributes(&self) -> &[NetworkPolicyAttributeValue] {
        &self.attributes
    }
}

/// Named network policy governing community creation and host placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkPolicy {
    id: Uuid,
    name: NetworkPolicyName,
    description: String,
    community_template_pattern: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NetworkPolicy {
    pub fn restore(
        id: Uuid,
        name: NetworkPolicyName,
        description: impl Into<String>,
        community_template_pattern: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            id,
            name,
            description: description.into(),
            community_template_pattern: validate_community_template_pattern(
                community_template_pattern,
            )?,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn name(&self) -> &NetworkPolicyName {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn community_template_pattern(&self) -> Option<&str> {
        self.community_template_pattern.as_deref()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new network policy.
#[derive(Clone, Debug)]
pub struct CreateNetworkPolicy {
    name: NetworkPolicyName,
    description: String,
    community_template_pattern: Option<String>,
    attributes: Vec<SetNetworkPolicyAttributeValue>,
}

impl CreateNetworkPolicy {
    pub fn new(
        name: NetworkPolicyName,
        description: impl Into<String>,
        community_template_pattern: Option<String>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            name,
            description: description.into(),
            community_template_pattern: validate_community_template_pattern(
                community_template_pattern,
            )?,
            attributes: Vec::new(),
        })
    }

    pub fn name(&self) -> &NetworkPolicyName {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn community_template_pattern(&self) -> Option<&str> {
        self.community_template_pattern.as_deref()
    }
    pub fn attributes(&self) -> &[SetNetworkPolicyAttributeValue] {
        &self.attributes
    }
    pub fn with_attributes(mut self, attributes: Vec<SetNetworkPolicyAttributeValue>) -> Self {
        self.attributes = attributes;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct UpdateNetworkPolicy {
    pub name: Option<NetworkPolicyName>,
    pub description: Option<String>,
    pub community_template_pattern: UpdateField<String>,
    /// `None` preserves memberships; `Some` replaces the complete set.
    pub attributes: Option<Vec<SetNetworkPolicyAttributeValue>>,
}

fn validate_community_template_pattern(value: Option<String>) -> Result<Option<String>, AppError> {
    value.map(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { return Ok(None); }
        if trimmed.len() > 100 || !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(AppError::validation(
                "community template pattern must contain only ASCII letters, digits, or underscores and be at most 100 characters",
            ));
        }
        Ok(Some(trimmed))
    }).unwrap_or(Ok(None))
}
