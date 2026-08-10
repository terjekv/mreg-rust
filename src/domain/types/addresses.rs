use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use ipnet::IpNet;
use macaddr::{MacAddr, MacAddr6, MacAddr8, ParseError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::AppError;

/// Length of a validated MAC address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MacAddressKind {
    Eui48,
    Eui64,
}

impl MacAddressKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eui48 => "eui48",
            Self::Eui64 => "eui64",
        }
    }
}

/// Validated 6-byte EUI-48 or 8-byte EUI-64 MAC address.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MacAddressValue(MacAddr);

impl MacAddressValue {
    #[inline(always)]
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let value = value.as_ref().trim();
        let parsed = match MacAddr6::from_str(value) {
            Ok(address) => MacAddr::V6(address),
            Err(error @ ParseError::InvalidCharacter(..)) => {
                return Err(AppError::validation(format!(
                    "invalid MAC address: {error}"
                )));
            }
            Err(ParseError::InvalidLength(_)) => MacAddr8::from_str(value)
                .map(MacAddr::V8)
                .map_err(|error| AppError::validation(format!("invalid MAC address: {error}")))?,
        };
        Ok(Self(parsed))
    }

    /// Returns the underlying EUI-48 or EUI-64 address.
    ///
    /// Use [`Self::as_eui48`] or [`Self::as_eui64`] when the caller requires a
    /// specific address width.
    pub fn as_inner(&self) -> MacAddr {
        self.0
    }

    pub fn kind(&self) -> MacAddressKind {
        match self.0 {
            MacAddr::V6(_) => MacAddressKind::Eui48,
            MacAddr::V8(_) => MacAddressKind::Eui64,
        }
    }

    pub fn as_eui48(&self) -> Option<MacAddr6> {
        match self.0 {
            MacAddr::V6(address) => Some(address),
            MacAddr::V8(_) => None,
        }
    }

    pub fn as_eui64(&self) -> Option<MacAddr8> {
        match self.0 {
            MacAddr::V6(_) => None,
            MacAddr::V8(address) => Some(address),
        }
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for MacAddressValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for MacAddressValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for MacAddressValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        MacAddressValue::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated IPv4 address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ipv4AddrValue(Ipv4Addr);

impl Ipv4AddrValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let parsed = value
            .as_ref()
            .trim()
            .parse::<Ipv4Addr>()
            .map_err(|error| AppError::validation(format!("invalid IPv4 address: {error}")))?;
        Ok(Self(parsed))
    }

    pub fn as_inner(&self) -> Ipv4Addr {
        self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for Ipv4AddrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Ipv4AddrValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for Ipv4AddrValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ipv4AddrValue::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated IPv6 address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ipv6AddrValue(Ipv6Addr);

impl Ipv6AddrValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let parsed = value
            .as_ref()
            .trim()
            .parse::<Ipv6Addr>()
            .map_err(|error| AppError::validation(format!("invalid IPv6 address: {error}")))?;
        Ok(Self(parsed))
    }

    pub fn as_inner(&self) -> Ipv6Addr {
        self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for Ipv6AddrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Ipv6AddrValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for Ipv6AddrValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ipv6AddrValue::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated IPv4 or IPv6 address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IpAddressValue(IpAddr);

impl IpAddressValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let parsed = value
            .as_ref()
            .trim()
            .parse::<IpAddr>()
            .map_err(|error| AppError::validation(format!("invalid IP address: {error}")))?;
        Ok(Self(parsed))
    }

    pub fn as_inner(&self) -> IpAddr {
        self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for IpAddressValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for IpAddressValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for IpAddressValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        IpAddressValue::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Convert an IP address to its PTR name.
///
/// For IPv4 (e.g., 192.168.1.10): "10.1.168.192.in-addr.arpa"
/// For IPv6 (e.g., fd00::1): expand to full 32-nibble hex, reverse nibbles,
/// join with dots, append ".ip6.arpa"
pub fn ip_to_ptr_name(address: &IpAddressValue) -> String {
    match address.as_inner() {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            format!(
                "{}.{}.{}.{}.in-addr.arpa",
                octets[3], octets[2], octets[1], octets[0]
            )
        }
        IpAddr::V6(v6) => {
            let octets = v6.octets();
            let mut nibbles = Vec::with_capacity(32);
            for byte in &octets {
                nibbles.push(byte >> 4);
                nibbles.push(byte & 0x0f);
            }
            nibbles.reverse();
            let parts: Vec<String> = nibbles.iter().map(|n| format!("{:x}", n)).collect();
            format!("{}.ip6.arpa", parts.join("."))
        }
    }
}

/// Validated CIDR network block.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CidrValue(IpNet);

impl CidrValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let parsed = value
            .as_ref()
            .trim()
            .parse::<IpNet>()
            .map_err(|error| AppError::validation(format!("invalid CIDR: {error}")))?;
        Ok(Self(parsed))
    }

    pub fn as_inner(&self) -> &IpNet {
        &self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }

    pub fn is_v6(&self) -> bool {
        matches!(self.0, IpNet::V6(_))
    }
}

impl fmt::Display for CidrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for CidrValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for CidrValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        CidrValue::new(raw).map_err(serde::de::Error::custom)
    }
}
