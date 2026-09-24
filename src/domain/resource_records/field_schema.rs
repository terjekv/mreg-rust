use serde::{Deserialize, Serialize};

use crate::errors::AppError;

/// Whether a record type allows single or multiple instances per owner.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordCardinality {
    Single,
    Multiple,
}

/// Data type for a field in a record type schema.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordFieldKind {
    Fqdn,
    DomainName,
    Ipv4,
    Ipv6,
    Uint16,
    Uint32,
    Float64,
    Enum,
    Text,
    CharString,
    Hex,
    List,
    Boolean,
}

/// Schema for a single field within a record type definition.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecordFieldSchema {
    name: String,
    kind: RecordFieldKind,
    required: bool,
    repeated: bool,
    options: Vec<String>,
}

impl RecordFieldSchema {
    pub fn new(
        name: impl Into<String>,
        kind: RecordFieldKind,
        required: bool,
        repeated: bool,
        options: Vec<String>,
    ) -> Result<Self, AppError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(AppError::validation("record field name cannot be empty"));
        }
        if !name
            .chars()
            .enumerate()
            .all(|(index, ch)| ch.is_ascii_alphanumeric() || ch == '_' && index > 0)
            || name.starts_with(|ch: char| ch.is_ascii_digit())
        {
            return Err(AppError::validation(
                "record field names must be ASCII identifiers",
            ));
        }
        if matches!(kind, RecordFieldKind::Enum) && options.is_empty() {
            return Err(AppError::validation(
                "enum record fields must define at least one option",
            ));
        }
        if !matches!(kind, RecordFieldKind::Enum) && !options.is_empty() {
            return Err(AppError::validation(
                "only enum record fields may define options",
            ));
        }
        let unique_options = options.iter().collect::<std::collections::BTreeSet<_>>();
        if unique_options.len() != options.len() || options.iter().any(String::is_empty) {
            return Err(AppError::validation(
                "record field options must be non-empty and unique",
            ));
        }

        Ok(Self {
            name,
            kind,
            required,
            repeated,
            options,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> &RecordFieldKind {
        &self.kind
    }

    pub fn required(&self) -> bool {
        self.required
    }

    pub fn repeated(&self) -> bool {
        self.repeated
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }
}
