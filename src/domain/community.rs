use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{CidrValue, CommunityName, NetworkPolicyName, RequiredDescription},
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
    description: RequiredDescription,
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
            description: RequiredDescription::new(description.into())?,
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
        self.description.as_str()
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
    description: RequiredDescription,
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
            description: RequiredDescription::new(description.into())?,
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
        self.description.as_str()
    }
}

/// Command to update a community without changing its network.
#[derive(Clone, Debug, Default)]
pub struct UpdateCommunity {
    pub name: Option<CommunityName>,
    pub description: Option<RequiredDescription>,
}

#[cfg(test)]
mod strictness_tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("")]
    #[case(" \t\n")]
    fn create_rejects_blank_description(#[case] value: &str) {
        assert!(
            CreateCommunity::new(
                NetworkPolicyName::new("campus").unwrap(),
                CidrValue::new("192.0.2.0/24").unwrap(),
                CommunityName::new("guests").unwrap(),
                value
            )
            .is_err()
        );
    }

    #[rstest]
    #[case("")]
    #[case(" \t\n")]
    fn restore_rejects_blank_description(#[case] value: &str) {
        let now = Utc::now();
        assert!(
            Community::restore(
                Uuid::nil(),
                Uuid::nil(),
                NetworkPolicyName::new("campus").unwrap(),
                CidrValue::new("192.0.2.0/24").unwrap(),
                CommunityName::new("guests").unwrap(),
                value,
                now,
                now
            )
            .is_err()
        );
    }
}
