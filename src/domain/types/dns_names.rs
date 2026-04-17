use std::fmt;

use hickory_proto::rr::Name;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::AppError;

/// Validated and normalized DNS name (lowercase, no trailing dot).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DnsName(String);

impl DnsName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let normalized = normalize_dns_name(value.as_ref())?;
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DnsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for DnsName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DnsName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        DnsName::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated DNS domain name that additionally allows the root domain (".").
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DomainNameValue(String);

impl DomainNameValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let trimmed = value.as_ref().trim();
        if trimmed == "." {
            return Ok(Self(".".to_string()));
        }
        Ok(Self(DnsName::new(trimmed)?.to_string()))
    }

    pub fn is_root(&self) -> bool {
        self.0 == "."
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DomainNameValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for DomainNameValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DomainNameValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        DomainNameValue::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated hostname (subset of DNS name, no underscores or wildcards).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hostname(DnsName);

impl Hostname {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let dns_name = DnsName::new(value)?;
        validate_hostname(dns_name.as_str())?;
        Ok(Self(dns_name))
    }

    pub fn as_dns_name(&self) -> &DnsName {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Hostname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for Hostname {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Hostname {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Hostname::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated DNS zone name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ZoneName(DnsName);

impl ZoneName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        Ok(Self(DnsName::new(value)?))
    }

    pub fn as_dns_name(&self) -> &DnsName {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ZoneName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for ZoneName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ZoneName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        ZoneName::new(raw).map_err(serde::de::Error::custom)
    }
}

fn normalize_dns_name(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("dns name cannot be empty"));
    }

    let normalized = trimmed.trim_end_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(AppError::validation("dns name cannot be root"));
    }

    let fqdn = format!("{normalized}.");
    Name::from_ascii(&fqdn)
        .map_err(|error| AppError::validation(format!("invalid dns name: {error}")))?;

    Ok(normalized)
}

fn validate_hostname(value: &str) -> Result<(), AppError> {
    if value.contains('*') {
        return Err(AppError::validation("hostname cannot contain '*'"));
    }
    if value.contains('_') {
        return Err(AppError::validation("hostname cannot contain '_'"));
    }

    for label in value.split('.') {
        if label.is_empty() {
            return Err(AppError::validation("hostname contains empty label"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(AppError::validation(
                "hostname labels cannot start or end with '-'",
            ));
        }
        if !label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(AppError::validation(
                "hostname labels must be alphanumeric or '-'",
            ));
        }
    }

    Ok(())
}
