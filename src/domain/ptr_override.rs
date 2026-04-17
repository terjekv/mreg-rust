use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::types::{DnsName, Hostname, IpAddressValue};

/// Override of the automatic reverse DNS (PTR) record for a host's IP address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PtrOverride {
    id: Uuid,
    host_name: Hostname,
    address: IpAddressValue,
    target_name: Option<DnsName>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PtrOverride {
    pub fn restore(
        id: Uuid,
        host_name: Hostname,
        address: IpAddressValue,
        target_name: Option<DnsName>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            host_name,
            address,
            target_name,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }
    pub fn address(&self) -> &IpAddressValue {
        &self.address
    }
    pub fn target_name(&self) -> Option<&DnsName> {
        self.target_name.as_ref()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a PTR override.
#[derive(Clone, Debug)]
pub struct CreatePtrOverride {
    host_name: Hostname,
    address: IpAddressValue,
    target_name: Option<DnsName>,
}

impl CreatePtrOverride {
    pub fn new(host_name: Hostname, address: IpAddressValue, target_name: Option<DnsName>) -> Self {
        Self {
            host_name,
            address,
            target_name,
        }
    }

    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }
    pub fn address(&self) -> &IpAddressValue {
        &self.address
    }
    pub fn target_name(&self) -> Option<&DnsName> {
        self.target_name.as_ref()
    }
}
