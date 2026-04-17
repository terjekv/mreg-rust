use std::{fmt, str::FromStr};

use email_address::EmailAddress;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::AppError;

/// Lowercase hex-encoded binary data (even number of digits).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HexEncodedValue(String);

impl HexEncodedValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let normalized = value.as_ref().trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err(AppError::validation("hex value cannot be empty"));
        }
        if normalized.len() % 2 != 0 {
            return Err(AppError::validation(
                "hex value must contain an even number of digits",
            ));
        }
        if !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(AppError::validation(
                "hex value must contain only hexadecimal digits",
            ));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HexEncodedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for HexEncodedValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for HexEncodedValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        HexEncodedValue::new(raw).map_err(serde::de::Error::custom)
    }
}

/// Validated and normalized email address (lowercase).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmailAddressValue(String);

impl EmailAddressValue {
    pub fn new(value: impl AsRef<str>) -> Result<Self, AppError> {
        let candidate = value.as_ref().trim().to_ascii_lowercase();
        EmailAddress::from_str(&candidate)
            .map_err(|error| AppError::validation(format!("invalid email address: {error}")))?;
        Ok(Self(candidate))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EmailAddressValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for EmailAddressValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EmailAddressValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        EmailAddressValue::new(raw).map_err(serde::de::Error::custom)
    }
}
