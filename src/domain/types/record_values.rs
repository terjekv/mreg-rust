use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::AppError;

/// Uppercase DNS record type name (e.g. "A", "AAAA", "CNAME").
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecordTypeName(String);

impl RecordTypeName {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let candidate = value.as_ref().trim().to_ascii_uppercase();
        if candidate.is_empty() {
            return Err(AppError::validation("record type name cannot be empty"));
        }
        if !candidate
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
        {
            return Err(AppError::validation(
                "record type name must contain only uppercase letters, digits, or '-'",
            ));
        }
        Ok(Self(candidate))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RecordTypeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for RecordTypeName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for RecordTypeName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        RecordTypeName::new(raw).map_err(serde::de::Error::custom)
    }
}

/// DNS character-string (max 255 bytes per RFC 1035).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DnsCharacterString(String);

impl DnsCharacterString {
    pub fn new(value: impl Into<String>) -> Result<Self, AppError> {
        let value = value.into();
        if value.len() > u8::MAX as usize {
            return Err(AppError::validation(
                "dns character-string cannot exceed 255 octets",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DnsCharacterString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for DnsCharacterString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DnsCharacterString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        DnsCharacterString::new(raw).map_err(serde::de::Error::custom)
    }
}
