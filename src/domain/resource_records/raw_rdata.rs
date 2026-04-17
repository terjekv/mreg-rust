use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{
        record_validation::{decode_hex_bytes, encode_hex_bytes},
        types::HexEncodedValue,
    },
    errors::AppError,
};

/// RFC 3597 wire-format RDATA with presentation format support.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawRdataValue {
    wire_bytes: Vec<u8>,
}

impl RawRdataValue {
    /// Parse from RFC 3597 presentation format: `\# <length> <hex>`.
    pub fn from_presentation(value: impl AsRef<str>) -> Result<Self, AppError> {
        let value = value.as_ref().trim();
        let Some(rest) = value.strip_prefix("\\#") else {
            return Err(AppError::validation(
                "raw RDATA must use RFC 3597 presentation format '\\# <length> <hex>'",
            ));
        };

        let parts = rest.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2 {
            return Err(AppError::validation(
                "raw RDATA must contain a length and hexadecimal payload",
            ));
        }

        let declared_len = parts[0].parse::<usize>().map_err(|error| {
            AppError::validation(format!("raw RDATA length is invalid: {error}"))
        })?;
        let hex = HexEncodedValue::new(parts[1])?;
        let wire_bytes = decode_hex_bytes(hex.as_str())?;
        if wire_bytes.len() != declared_len {
            return Err(AppError::validation(format!(
                "raw RDATA length {} does not match {} decoded bytes",
                declared_len,
                wire_bytes.len()
            )));
        }

        Ok(Self { wire_bytes })
    }

    /// Construct directly from wire-format bytes.
    pub fn from_wire_bytes(wire_bytes: Vec<u8>) -> Result<Self, AppError> {
        Ok(Self { wire_bytes })
    }

    /// Render as RFC 3597 presentation format string.
    pub fn presentation(&self) -> String {
        format!(
            "\\# {} {}",
            self.wire_bytes.len(),
            encode_hex_bytes(&self.wire_bytes)
        )
    }

    pub fn wire_bytes(&self) -> &[u8] {
        &self.wire_bytes
    }
}

/// Result of record input validation: either structured JSON or raw wire-format RDATA.
#[derive(Clone, Debug)]
pub enum ValidatedRecordContent {
    Structured(Value),
    RawRdata(RawRdataValue),
}
