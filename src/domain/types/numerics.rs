use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::AppError;

/// DNS time-to-live value (0 to i32::MAX seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ttl(u32);

impl Ttl {
    pub fn new(value: u32) -> Result<Self, AppError> {
        if value > i32::MAX as u32 {
            return Err(AppError::validation("ttl exceeds supported range"));
        }
        Ok(Self(value))
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}

impl Serialize for Ttl {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for Ttl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u32::deserialize(deserializer)?;
        Ttl::new(raw).map_err(serde::de::Error::custom)
    }
}

/// DNS SOA serial number with RFC 1912 YYYYMMDDNNNN increment support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SerialNumber(u64);

impl SerialNumber {
    pub fn new(value: u64) -> Result<Self, AppError> {
        if value > i64::MAX as u64 {
            return Err(AppError::validation(
                "serial number exceeds supported range",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }

    pub fn as_i64(&self) -> i64 {
        self.0 as i64
    }

    /// Compute the next serial using YYYYMMDDNNNN format.
    ///
    /// Uses 4 digits for the daily counter (0000–9999), allowing up to 10,000
    /// zone changes per day. This extends the common RFC 1912 YYYYMMDDNN
    /// convention while staying well within the 32-bit serial range.
    ///
    /// If the current serial starts with today's YYYYMMDD prefix and NNNN < 9999,
    /// increments NNNN. Otherwise, starts at YYYYMMDD0000. If the result would be
    /// less than or equal to the current serial (e.g. clock skew), adds 1 to
    /// the current serial instead.
    pub fn next_rfc1912(&self, today: chrono::NaiveDate) -> Result<Self, AppError> {
        let prefix = today.format("%Y%m%d").to_string().parse::<u64>().unwrap() * 10_000;
        let next = if self.0 >= prefix && self.0 < prefix + 9999 {
            self.0 + 1
        } else if prefix > self.0 {
            prefix
        } else {
            // Clock skew or daily counter exhausted: just increment
            self.0 + 1
        };
        Self::new(next)
    }
}

impl Serialize for SerialNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for SerialNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u64::deserialize(deserializer)?;
        SerialNumber::new(raw).map_err(serde::de::Error::custom)
    }
}

/// BACnet device identifier (positive u32).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BacnetIdentifier(u32);

impl BacnetIdentifier {
    pub fn new(value: u32) -> Result<Self, AppError> {
        if value == 0 {
            return Err(AppError::validation("bacnet identifier must be positive"));
        }
        Ok(Self(value))
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn as_i32(&self) -> i32 {
        self.0 as i32
    }
}

impl Serialize for BacnetIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl<'de> Deserialize<'de> for BacnetIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u32::deserialize(deserializer)?;
        BacnetIdentifier::new(raw).map_err(serde::de::Error::custom)
    }
}
