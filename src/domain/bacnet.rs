use chrono::{DateTime, Utc};

use crate::domain::types::{BacnetIdentifier, Hostname};

/// Mapping of a BACnet device identifier to a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacnetIdAssignment {
    bacnet_id: BacnetIdentifier,
    host_name: Hostname,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl BacnetIdAssignment {
    pub fn restore(
        bacnet_id: BacnetIdentifier,
        host_name: Hostname,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            bacnet_id,
            host_name,
            created_at,
            updated_at,
        }
    }

    pub fn bacnet_id(&self) -> BacnetIdentifier {
        self.bacnet_id
    }
    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to assign a BACnet ID to a host.
#[derive(Clone, Debug)]
pub struct CreateBacnetIdAssignment {
    bacnet_id: BacnetIdentifier,
    host_name: Hostname,
}

impl CreateBacnetIdAssignment {
    pub fn new(bacnet_id: BacnetIdentifier, host_name: Hostname) -> Self {
        Self {
            bacnet_id,
            host_name,
        }
    }

    pub fn bacnet_id(&self) -> BacnetIdentifier {
        self.bacnet_id
    }
    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }
}
