use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::{DnsTypeCode, RecordTypeName};

pub(super) fn builtin_loc() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("LOC")?,
        Some(DnsTypeCode::new(29)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Single,
            false,
            vec![
                RecordFieldSchema::new(
                    "latitude",
                    RecordFieldKind::Float64,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "longitude",
                    RecordFieldKind::Float64,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "altitude_m",
                    RecordFieldKind::Float64,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "size_m",
                    RecordFieldKind::Float64,
                    false,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "horizontal_precision_m",
                    RecordFieldKind::Float64,
                    false,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "vertical_precision_m",
                    RecordFieldKind::Float64,
                    false,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1876"],
                    "owner_name_syntax": "hostname",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ latitude }} {{ longitude }} {{ altitude_m }}m {% if size_m is defined %}{{ size_m }}m{% endif %}".to_string()),
        )?,
        true,
    ))
}

/// URI (RFC 7553) — uniform resource identifier for service discovery.
pub(super) fn builtin_uri() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("URI")?,
        Some(DnsTypeCode::new(256)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new(
                    "priority",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("weight", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new("target", RecordFieldKind::Text, true, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC7553"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ priority }} {{ weight }} \"{{ target }}\"".to_string()),
        )?,
        true,
    ))
}
