use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    domain::types::{
        CidrValue, Hostname, IpAddressValue, MacAddressValue, Ttl, UpdateField, ZoneName,
    },
    errors::AppError,
};

/// DNS host entry with optional zone membership and TTL override.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    id: Uuid,
    name: Hostname,
    zone: Option<ZoneName>,
    ttl: Option<Ttl>,
    comment: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Host {
    pub fn restore(
        id: Uuid,
        name: Hostname,
        zone: Option<ZoneName>,
        ttl: Option<Ttl>,
        comment: impl Into<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let comment = comment.into().trim().to_string();
        validate_zone_membership(&name, zone.as_ref())?;

        Ok(Self {
            id,
            name,
            zone,
            ttl,
            comment,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &Hostname {
        &self.name
    }

    pub fn zone(&self) -> Option<&ZoneName> {
        self.zone.as_ref()
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn comment(&self) -> &str {
        &self.comment
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Host facts assembled specifically for authorization decisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostAuthContext {
    host: Host,
    addresses: Vec<IpAddressValue>,
    networks: Vec<CidrValue>,
}

impl HostAuthContext {
    pub fn new(host: Host, addresses: Vec<IpAddressValue>, networks: Vec<CidrValue>) -> Self {
        Self {
            host,
            addresses,
            networks,
        }
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn addresses(&self) -> &[IpAddressValue] {
        &self.addresses
    }

    pub fn networks(&self) -> &[CidrValue] {
        &self.networks
    }
}

/// Command to create a new host, validating zone membership.
/// Optionally includes IP assignment specs for atomic host+IP creation.
#[derive(Clone, Debug)]
pub struct CreateHost {
    name: Hostname,
    zone: Option<ZoneName>,
    ttl: Option<Ttl>,
    comment: String,
    ip_assignments: Vec<IpAssignmentSpec>,
}

impl CreateHost {
    pub fn new(
        name: Hostname,
        zone: Option<ZoneName>,
        ttl: Option<Ttl>,
        comment: impl Into<String>,
    ) -> Result<Self, AppError> {
        validate_zone_membership(&name, zone.as_ref())?;
        Ok(Self {
            name,
            zone,
            ttl,
            comment: comment.into().trim().to_string(),
            ip_assignments: Vec::new(),
        })
    }

    pub fn with_ip_assignments(mut self, specs: Vec<IpAssignmentSpec>) -> Self {
        self.ip_assignments = specs;
        self
    }

    pub fn name(&self) -> &Hostname {
        &self.name
    }

    pub fn zone(&self) -> Option<&ZoneName> {
        self.zone.as_ref()
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn comment(&self) -> &str {
        &self.comment
    }

    pub fn ip_assignments(&self) -> &[IpAssignmentSpec] {
        &self.ip_assignments
    }
}

/// Association between a host and an IP address within a network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpAddressAssignment {
    id: Uuid,
    host_id: Uuid,
    attachment_id: Uuid,
    address: IpAddressValue,
    family: u8,
    network_id: Uuid,
    mac_address: Option<MacAddressValue>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl IpAddressAssignment {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        host_id: Uuid,
        attachment_id: Uuid,
        address: IpAddressValue,
        network_id: Uuid,
        mac_address: Option<MacAddressValue>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let family = match address.as_inner() {
            std::net::IpAddr::V4(_) => 4,
            std::net::IpAddr::V6(_) => 6,
        };

        Ok(Self {
            id,
            host_id,
            attachment_id,
            address,
            family,
            network_id,
            mac_address,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    pub fn address(&self) -> &IpAddressValue {
        &self.address
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn family(&self) -> u8 {
        self.family
    }

    pub fn network_id(&self) -> Uuid {
        self.network_id
    }

    pub fn mac_address(&self) -> Option<&MacAddressValue> {
        self.mac_address.as_ref()
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Policy for how an IP address is selected from a network during auto-allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum AllocationPolicy {
    /// Select the first available address in the network (lowest usable).
    #[default]
    FirstFree,
    /// Select a random available address in the network.
    Random,
}

/// Specification for an IP address to assign during host creation.
#[derive(Clone, Debug)]
pub struct IpAssignmentSpec {
    address: Option<IpAddressValue>,
    network: Option<crate::domain::types::CidrValue>,
    allocation: AllocationPolicy,
    mac_address: Option<MacAddressValue>,
    auto_v4_client_id: bool,
    auto_v6_duid_ll: bool,
}

impl IpAssignmentSpec {
    pub fn new(
        address: Option<IpAddressValue>,
        network: Option<crate::domain::types::CidrValue>,
        allocation: AllocationPolicy,
        mac_address: Option<MacAddressValue>,
    ) -> Result<Self, AppError> {
        if address.is_none() && network.is_none() {
            return Err(AppError::validation(
                "each ip_addresses entry must specify either address or network",
            ));
        }
        Ok(Self {
            address,
            network,
            allocation,
            mac_address,
            auto_v4_client_id: false,
            auto_v6_duid_ll: false,
        })
    }

    pub fn with_auto_dhcp(mut self, v4: bool, v6: bool) -> Self {
        self.auto_v4_client_id = v4;
        self.auto_v6_duid_ll = v6;
        self
    }

    pub fn into_assign_command(self, host_name: Hostname) -> Result<AssignIpAddress, AppError> {
        let cmd = AssignIpAddress::new(host_name, self.address, self.network, self.mac_address)?;
        Ok(cmd.with_auto_dhcp(self.auto_v4_client_id, self.auto_v6_duid_ll))
    }

    pub fn address(&self) -> Option<&IpAddressValue> {
        self.address.as_ref()
    }

    pub fn network(&self) -> Option<&crate::domain::types::CidrValue> {
        self.network.as_ref()
    }

    pub fn allocation(&self) -> &AllocationPolicy {
        &self.allocation
    }

    pub fn mac_address(&self) -> Option<&MacAddressValue> {
        self.mac_address.as_ref()
    }

    pub fn auto_v4_client_id(&self) -> bool {
        self.auto_v4_client_id
    }

    pub fn auto_v6_duid_ll(&self) -> bool {
        self.auto_v6_duid_ll
    }
}

/// Command to assign an IP address to a host, either explicitly or by network auto-allocation.
#[derive(Clone, Debug)]
pub struct AssignIpAddress {
    host_name: Hostname,
    address: Option<IpAddressValue>,
    network: Option<crate::domain::types::CidrValue>,
    mac_address: Option<MacAddressValue>,
    auto_v4_client_id: bool,
    auto_v6_duid_ll: bool,
}

impl AssignIpAddress {
    pub fn new(
        host_name: Hostname,
        address: Option<IpAddressValue>,
        network: Option<crate::domain::types::CidrValue>,
        mac_address: Option<MacAddressValue>,
    ) -> Result<Self, AppError> {
        if address.is_none() && network.is_none() {
            return Err(AppError::validation(
                "either an explicit address or a network must be provided",
            ));
        }

        Ok(Self {
            host_name,
            address,
            network,
            mac_address,
            auto_v4_client_id: false,
            auto_v6_duid_ll: false,
        })
    }

    pub fn with_auto_dhcp(mut self, v4: bool, v6: bool) -> Self {
        self.auto_v4_client_id = v4;
        self.auto_v6_duid_ll = v6;
        self
    }

    pub fn host_name(&self) -> &Hostname {
        &self.host_name
    }

    pub fn address(&self) -> Option<&IpAddressValue> {
        self.address.as_ref()
    }

    pub fn network(&self) -> Option<&crate::domain::types::CidrValue> {
        self.network.as_ref()
    }

    pub fn mac_address(&self) -> Option<&MacAddressValue> {
        self.mac_address.as_ref()
    }

    pub fn auto_v4_client_id(&self) -> bool {
        self.auto_v4_client_id
    }

    pub fn auto_v6_duid_ll(&self) -> bool {
        self.auto_v6_duid_ll
    }
}

fn validate_zone_membership(name: &Hostname, zone: Option<&ZoneName>) -> Result<(), AppError> {
    if let Some(zone) = zone {
        let host_name = name.as_str();
        let zone_name = zone.as_str();
        if host_name != zone_name && !host_name.ends_with(&format!(".{zone_name}")) {
            return Err(AppError::validation(format!(
                "host '{}' does not belong to zone '{}'",
                host_name, zone_name
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::domain::types::{Hostname, ZoneName};

    use super::CreateHost;

    #[test]
    fn host_rejects_zone_mismatch() {
        let result = CreateHost::new(
            Hostname::new("app.example.org").expect("valid hostname"),
            Some(ZoneName::new("other.org").expect("valid zone")),
            None,
            "test host",
        );

        assert!(result.is_err());
    }
}

/// Command to update a host's name, TTL, comment, or zone.
#[derive(Clone, Debug)]
pub struct UpdateHost {
    pub name: Option<Hostname>,
    pub ttl: UpdateField<Ttl>,
    pub comment: Option<String>,
    pub zone: UpdateField<ZoneName>,
}

/// Command to update an IP address assignment (currently just MAC address).
#[derive(Clone, Debug)]
pub struct UpdateIpAddress {
    pub mac_address: UpdateField<MacAddressValue>,
}
