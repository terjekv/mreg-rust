use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::AppError;

/// Lowercase alphanumeric label name (hyphens and underscores allowed).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LabelName(String);

impl LabelName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        Ok(Self(normalize_identifier_name(
            value.as_ref(),
            "label name",
            false,
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LabelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for LabelName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LabelName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        LabelName::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated host-policy atom or role name (lowercase letters, digits, hyphens, underscores).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostPolicyName(String);

impl HostPolicyName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        Ok(Self(normalize_identifier_name(
            value.as_ref(),
            "host policy name",
            false,
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validated host group name (lowercase letters, digits, hyphens, underscores, dots).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HostGroupName(String);

impl HostGroupName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        Ok(Self(normalize_identifier_name(
            value.as_ref(),
            "host group name",
            true,
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostGroupName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for HostGroupName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HostGroupName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        HostGroupName::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated network policy name (lowercase, normalized).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NetworkPolicyName(String);

impl NetworkPolicyName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        Ok(Self(normalize_identifier_name(
            value.as_ref(),
            "network policy name",
            true,
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NetworkPolicyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for NetworkPolicyName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for NetworkPolicyName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        NetworkPolicyName::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated community name (lowercase, normalized).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommunityName(String);

impl CommunityName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        Ok(Self(normalize_identifier_name(
            value.as_ref(),
            "community name",
            true,
        )?))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommunityName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for CommunityName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CommunityName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        CommunityName::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated owner group name (non-empty, no whitespace).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OwnerGroupName(String);

impl OwnerGroupName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let candidate = value.as_ref().trim().to_string();
        if candidate.is_empty() {
            return Err(AppError::validation("owner group name cannot be empty"));
        }
        if candidate.chars().any(char::is_whitespace) {
            return Err(AppError::validation(
                "owner group name cannot contain whitespace",
            ));
        }
        Ok(Self(candidate))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OwnerGroupName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for OwnerGroupName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OwnerGroupName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        OwnerGroupName::new(raw).map_err(serde::de::Error::custom)
    }
}

fn normalize_identifier_name(
    value: &str,
    label: &str,
    allow_dots: bool,
) -> Result<String, AppError> {
    let candidate = value.trim().to_ascii_lowercase();
    if candidate.is_empty() {
        return Err(AppError::validation(format!("{label} cannot be empty")));
    }
    let valid = candidate.chars().all(|ch| {
        ch.is_ascii_lowercase()
            || ch.is_ascii_digit()
            || ch == '-'
            || ch == '_'
            || (allow_dots && ch == '.')
    });
    if !valid {
        let allowed = if allow_dots {
            "lowercase letters, digits, '.', '-' or '_'"
        } else {
            "lowercase letters, digits, '-' or '_'"
        };
        return Err(AppError::validation(format!(
            "{label} must contain only {allowed}"
        )));
    }
    Ok(candidate)
}
