use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{CidrValue, CommunityName, NetworkPolicyName},
    errors::AppError,
};

/// Named community within a network policy, scoped to a specific network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Community {
    id: Uuid,
    policy_id: Uuid,
    policy_name: NetworkPolicyName,
    network_cidr: CidrValue,
    name: CommunityName,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Community {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        policy_id: Uuid,
        policy_name: NetworkPolicyName,
        network_cidr: CidrValue,
        name: CommunityName,
        description: impl Into<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            id,
            policy_id,
            policy_name,
            network_cidr,
            name,
            description: normalize_required_text(description.into(), "community description")?,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn policy_id(&self) -> Uuid {
        self.policy_id
    }
    pub fn policy_name(&self) -> &NetworkPolicyName {
        &self.policy_name
    }
    pub fn network_cidr(&self) -> &CidrValue {
        &self.network_cidr
    }
    pub fn name(&self) -> &CommunityName {
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

/// Command to create a new community under a network policy.
#[derive(Clone, Debug)]
pub struct CreateCommunity {
    policy_name: NetworkPolicyName,
    network_cidr: CidrValue,
    name: CommunityName,
    description: String,
}

impl CreateCommunity {
    pub fn new(
        policy_name: NetworkPolicyName,
        network_cidr: CidrValue,
        name: CommunityName,
        description: impl Into<String>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            policy_name,
            network_cidr,
            name,
            description: normalize_required_text(description.into(), "community description")?,
        })
    }

    pub fn policy_name(&self) -> &NetworkPolicyName {
        &self.policy_name
    }
    pub fn network_cidr(&self) -> &CidrValue {
        &self.network_cidr
    }
    pub fn name(&self) -> &CommunityName {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
}

fn normalize_required_text(value: String, label: &str) -> Result<String, AppError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::validation(format!("{label} cannot be empty")));
    }
    Ok(trimmed)
}
