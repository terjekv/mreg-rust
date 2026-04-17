use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::{DnsTypeCode, RecordTypeName};

pub(super) fn builtin_tlsa() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("TLSA")?,
        Some(DnsTypeCode::new(52)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new("usage", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new(
                    "selector",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "matching_type",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "certificate_data",
                    RecordFieldKind::Hex,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC6698", "RFC7671"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some(
                "{{ usage }} {{ selector }} {{ matching_type }} {{ certificate_data }}".to_string(),
            ),
        )?,
        true,
    ))
}

pub(super) fn builtin_svcb() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("SVCB")?,
        Some(DnsTypeCode::new(64)?),
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
                RecordFieldSchema::new(
                    "target",
                    RecordFieldKind::DomainName,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("params", RecordFieldKind::Text, false, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC9460"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["target"],
                    "supports_null_domain_target": true
                }
            }),
            Some(
                "{{ priority }} {{ target }}{% if params is defined %} {{ params }}{% endif %}"
                    .to_string(),
            ),
        )?,
        true,
    ))
}

pub(super) fn builtin_https() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("HTTPS")?,
        Some(DnsTypeCode::new(65)?),
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
                RecordFieldSchema::new(
                    "target",
                    RecordFieldKind::DomainName,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("params", RecordFieldKind::Text, false, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC9460"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["target"],
                    "supports_null_domain_target": true
                }
            }),
            Some(
                "{{ priority }} {{ target }}{% if params is defined %} {{ params }}{% endif %}"
                    .to_string(),
            ),
        )?,
        true,
    ))
}

/// OPENPGPKEY (RFC 7929) — OpenPGP public key for DANE.
pub(super) fn builtin_openpgpkey() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("OPENPGPKEY")?,
        Some(DnsTypeCode::new(61)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![RecordFieldSchema::new(
                "public_key",
                RecordFieldKind::Text,
                true,
                false,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC7929"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ public_key }}".to_string()),
        )?,
        true,
    ))
}

/// SMIMEA (RFC 8162) — S/MIME certificate association, like TLSA for email.
pub(super) fn builtin_smimea() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("SMIMEA")?,
        Some(DnsTypeCode::new(53)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new("usage", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new(
                    "selector",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "matching_type",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "certificate_data",
                    RecordFieldKind::Hex,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC8162"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some(
                "{{ usage }} {{ selector }} {{ matching_type }} {{ certificate_data }}".to_string(),
            ),
        )?,
        true,
    ))
}
