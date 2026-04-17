use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{domain::types::NetworkPolicyName, errors::AppError};

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
            description: normalize_required_text(description.into(), "network policy description")?,
            community_template_pattern: normalize_optional_text(community_template_pattern),
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
}

impl CreateNetworkPolicy {
    pub fn new(
        name: NetworkPolicyName,
        description: impl Into<String>,
        community_template_pattern: Option<String>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            name,
            description: normalize_required_text(description.into(), "network policy description")?,
            community_template_pattern: normalize_optional_text(community_template_pattern),
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
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn normalize_required_text(value: String, label: &str) -> Result<String, AppError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::validation(format!("{label} cannot be empty")));
    }
    Ok(trimmed)
}
