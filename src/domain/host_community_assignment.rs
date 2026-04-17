use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::types::{CommunityName, Hostname, IpAddressValue, NetworkPolicyName};

/// Association between a host's IP address and a community within a network policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCommunityAssignment {
    id: Uuid,
    host_id: Uuid,
    host_name: Hostname,
    ip_address_id: Uuid,
    address: IpAddressValue,
    community_id: Uuid,
    community_name: CommunityName,
    policy_name: NetworkPolicyName,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostCommunityAssignment {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        host_id: Uuid,
        host_name: Hostname,
        ip_address_id: Uuid,
        address: IpAddressValue,
        community_id: Uuid,
        community_name: CommunityName,
        policy_name: NetworkPolicyName,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            host_id,
            host_name,
            ip_address_id,
            address,
            community_id,
            community_name,
            policy_name,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn host_id(&self) -> Uuid {
        self.host_id
    }
    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }
    pub fn ip_address_id(&self) -> Uuid {
        self.ip_address_id
    }
    pub fn address(&self) -> &IpAddressValue {
        &self.address
    }
    pub fn community_id(&self) -> Uuid {
        self.community_id
    }
    pub fn community_name(&self) -> &CommunityName {
        &self.community_name
    }
    pub fn policy_name(&self) -> &NetworkPolicyName {
        &self.policy_name
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to map a host's IP address to a community.
#[derive(Clone, Debug)]
pub struct CreateHostCommunityAssignment {
    host_name: Hostname,
    address: IpAddressValue,
    policy_name: NetworkPolicyName,
    community_name: CommunityName,
}

impl CreateHostCommunityAssignment {
    pub fn new(
        host_name: Hostname,
        address: IpAddressValue,
        policy_name: NetworkPolicyName,
        community_name: CommunityName,
    ) -> Self {
        Self {
            host_name,
            address,
            policy_name,
            community_name,
        }
    }

    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }
    pub fn address(&self) -> &IpAddressValue {
        &self.address
    }
    pub fn policy_name(&self) -> &NetworkPolicyName {
        &self.policy_name
    }
    pub fn community_name(&self) -> &CommunityName {
        &self.community_name
    }
}
