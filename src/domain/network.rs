use std::net::IpAddr;

use chrono::{DateTime, Utc};
use ipnet::IpNet;
use uuid::Uuid;

use crate::{
    domain::types::{CidrValue, IpAddressValue, ReservedCount, UpdateField, VlanId},
    errors::AppError,
};

/// IP network (IPv4 or IPv6) defined by a CIDR block with a reserved address count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    id: Uuid,
    cidr: CidrValue,
    description: String,
    vlan: Option<VlanId>,
    dns_delegated: bool,
    category: String,
    location: String,
    frozen: bool,
    reserved: ReservedCount,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Network {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        cidr: CidrValue,
        description: impl Into<String>,
        vlan: Option<VlanId>,
        dns_delegated: bool,
        category: impl Into<String>,
        location: impl Into<String>,
        frozen: bool,
        reserved: ReservedCount,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(AppError::validation("network description cannot be empty"));
        }

        Ok(Self {
            id,
            cidr,
            description,
            vlan,
            dns_delegated,
            category: category.into(),
            location: location.into(),
            frozen,
            reserved,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn cidr(&self) -> &CidrValue {
        &self.cidr
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn vlan(&self) -> Option<VlanId> {
        self.vlan
    }

    pub fn dns_delegated(&self) -> bool {
        self.dns_delegated
    }

    pub fn category(&self) -> &str {
        &self.category
    }

    pub fn location(&self) -> &str {
        &self.location
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    pub fn reserved(&self) -> ReservedCount {
        self.reserved
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn contains(&self, address: &IpAddressValue) -> bool {
        self.cidr.as_inner().contains(&address.as_inner())
    }

    pub fn prefix_len(&self) -> u8 {
        self.cidr.as_inner().prefix_len()
    }
}

/// Command to create a new network.
#[derive(Clone, Debug)]
pub struct CreateNetwork {
    cidr: CidrValue,
    description: String,
    vlan: Option<VlanId>,
    dns_delegated: bool,
    category: String,
    location: String,
    frozen: bool,
    reserved: ReservedCount,
}

impl CreateNetwork {
    pub fn new(
        cidr: CidrValue,
        description: impl Into<String>,
        reserved: ReservedCount,
    ) -> Result<Self, AppError> {
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(AppError::validation("network description cannot be empty"));
        }

        Ok(Self {
            cidr,
            description,
            vlan: None,
            dns_delegated: false,
            category: String::new(),
            location: String::new(),
            frozen: false,
            reserved,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_full(
        cidr: CidrValue,
        description: impl Into<String>,
        vlan: Option<VlanId>,
        dns_delegated: bool,
        category: impl Into<String>,
        location: impl Into<String>,
        frozen: bool,
        reserved: ReservedCount,
    ) -> Result<Self, AppError> {
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(AppError::validation("network description cannot be empty"));
        }

        Ok(Self {
            cidr,
            description,
            vlan,
            dns_delegated,
            category: category.into(),
            location: location.into(),
            frozen,
            reserved,
        })
    }

    pub fn cidr(&self) -> &CidrValue {
        &self.cidr
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn vlan(&self) -> Option<VlanId> {
        self.vlan
    }

    pub fn dns_delegated(&self) -> bool {
        self.dns_delegated
    }

    pub fn category(&self) -> &str {
        &self.category
    }

    pub fn location(&self) -> &str {
        &self.location
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    pub fn reserved(&self) -> ReservedCount {
        self.reserved
    }
}

/// Command to update an existing network.
#[derive(Clone, Debug)]
pub struct UpdateNetwork {
    pub description: Option<String>,
    pub vlan: UpdateField<VlanId>,
    pub dns_delegated: Option<bool>,
    pub category: Option<String>,
    pub location: Option<String>,
    pub frozen: Option<bool>,
    pub reserved: Option<ReservedCount>,
}

/// Contiguous IP range excluded from automatic address allocation within a network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExcludedRange {
    id: Uuid,
    network_id: Uuid,
    start_ip: IpAddressValue,
    end_ip: IpAddressValue,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ExcludedRange {
    pub fn restore(
        id: Uuid,
        network_id: Uuid,
        start_ip: IpAddressValue,
        end_ip: IpAddressValue,
        description: impl Into<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        validate_ip_order(&start_ip, &end_ip)?;
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(AppError::validation(
                "excluded range description cannot be empty",
            ));
        }

        Ok(Self {
            id,
            network_id,
            start_ip,
            end_ip,
            description,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn network_id(&self) -> Uuid {
        self.network_id
    }

    pub fn start_ip(&self) -> &IpAddressValue {
        &self.start_ip
    }

    pub fn end_ip(&self) -> &IpAddressValue {
        &self.end_ip
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

    pub fn contains(&self, address: &IpAddressValue) -> bool {
        same_family(&self.start_ip.as_inner(), &address.as_inner())
            && ip_to_u128(self.start_ip.as_inner()) <= ip_to_u128(address.as_inner())
            && ip_to_u128(address.as_inner()) <= ip_to_u128(self.end_ip.as_inner())
    }
}

/// Command to add an excluded IP range to a network.
#[derive(Clone, Debug)]
pub struct CreateExcludedRange {
    start_ip: IpAddressValue,
    end_ip: IpAddressValue,
    description: String,
}

impl CreateExcludedRange {
    pub fn new(
        start_ip: IpAddressValue,
        end_ip: IpAddressValue,
        description: impl Into<String>,
    ) -> Result<Self, AppError> {
        validate_ip_order(&start_ip, &end_ip)?;
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(AppError::validation(
                "excluded range description cannot be empty",
            ));
        }

        Ok(Self {
            start_ip,
            end_ip,
            description,
        })
    }

    pub fn start_ip(&self) -> &IpAddressValue {
        &self.start_ip
    }

    pub fn end_ip(&self) -> &IpAddressValue {
        &self.end_ip
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Convert an IP address to a u128 for range comparisons (IPv4 uses the low 32 bits).
pub fn ip_to_u128(ip: IpAddr) -> u128 {
    match ip {
        IpAddr::V4(value) => u32::from(value) as u128,
        IpAddr::V6(value) => u128::from(value),
    }
}

/// Check whether two IP addresses belong to the same address family.
pub fn same_family(left: &IpAddr, right: &IpAddr) -> bool {
    matches!(
        (left, right),
        (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_))
    )
}

/// Validate that start_ip <= end_ip and both use the same address family.
pub fn validate_ip_order(
    start_ip: &IpAddressValue,
    end_ip: &IpAddressValue,
) -> Result<(), AppError> {
    if !same_family(&start_ip.as_inner(), &end_ip.as_inner()) {
        return Err(AppError::validation(
            "excluded range start and end must use the same IP family",
        ));
    }

    if ip_to_u128(start_ip.as_inner()) > ip_to_u128(end_ip.as_inner()) {
        return Err(AppError::validation(
            "excluded range start IP must be less than or equal to end IP",
        ));
    }

    Ok(())
}

/// Check whether an IP address falls within a CIDR network block.
pub fn cidr_contains(net: &CidrValue, ip: &IpAddressValue) -> bool {
    net.as_inner().contains(&ip.as_inner())
}

/// Compute the usable (first, last) address bounds of a network after reserved space.
pub fn network_usable_bounds(
    net: &CidrValue,
    reserved: ReservedCount,
) -> Result<(u128, u128), AppError> {
    match net.as_inner() {
        IpNet::V4(v4) => {
            let network = u32::from(v4.network()) as u128;
            let broadcast = u32::from(v4.broadcast()) as u128;
            let first = network.saturating_add(reserved.as_u32() as u128);
            let last = broadcast.saturating_sub(1);
            if first > last {
                return Err(AppError::validation(
                    "network has no allocatable IPv4 addresses after reserved space",
                ));
            }
            Ok((first, last))
        }
        IpNet::V6(v6) => {
            let network = u128::from(v6.network());
            let first = network.saturating_add(reserved.as_u32() as u128);
            let host_bits = 128u32.saturating_sub(v6.prefix_len() as u32);
            if host_bits > 20 {
                return Err(AppError::validation(
                    "automatic IPv6 allocation is only supported for relatively small prefixes",
                ));
            }
            let size = 1u128
                .checked_shl(host_bits)
                .ok_or_else(|| AppError::validation("unsupported IPv6 allocation size"))?;
            let last = network + size - 1;
            if first > last {
                return Err(AppError::validation(
                    "network has no allocatable IPv6 addresses after reserved space",
                ));
            }
            Ok((first, last))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CreateExcludedRange, CreateNetwork, ip_to_u128};
    use crate::domain::types::{CidrValue, IpAddressValue};

    #[test]
    fn excluded_range_rejects_reversed_addresses() {
        let result = CreateExcludedRange::new(
            IpAddressValue::new("10.0.0.20").expect("valid IP"),
            IpAddressValue::new("10.0.0.10").expect("valid IP"),
            "bad range",
        );
        assert!(result.is_err());
    }

    #[test]
    fn network_creation_requires_description() {
        let result = CreateNetwork::new(
            CidrValue::new("10.0.0.0/24").expect("valid CIDR"),
            " ",
            crate::domain::types::ReservedCount::new(3).unwrap(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn ip_to_u128_supports_ipv4() {
        let value = ip_to_u128(
            IpAddressValue::new("10.0.0.10")
                .expect("valid IP")
                .as_inner(),
        );
        assert_eq!(value, 167_772_170);
    }
}
