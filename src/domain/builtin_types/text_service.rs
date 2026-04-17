use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::{DnsTypeCode, RecordTypeName};

pub(super) fn builtin_txt() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("TXT")?,
        Some(DnsTypeCode::new(16)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![RecordFieldSchema::new(
                "value",
                RecordFieldKind::CharString,
                true,
                true,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1035"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some(
                "{% for chunk in value %}\"{{ chunk }}\"{% if not loop.last %} {% endif %}{% endfor %}"
                    .to_string(),
            ),
        )?,
        true,
    ))
}

pub(super) fn builtin_srv() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("SRV")?,
        Some(DnsTypeCode::new(33)?),
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
                RecordFieldSchema::new("port", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new(
                    "target",
                    RecordFieldKind::DomainName,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC2782", "RFC2181"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["target"],
                    "supports_null_domain_target": true
                }
            }),
            Some("{{ priority }} {{ weight }} {{ port }} {{ target }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_naptr() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("NAPTR")?,
        Some(DnsTypeCode::new(35)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new(
                    "order",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "preference",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "flags",
                    RecordFieldKind::CharString,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "services",
                    RecordFieldKind::CharString,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "regexp",
                    RecordFieldKind::CharString,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "replacement",
                    RecordFieldKind::DomainName,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC3403"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["replacement"],
                    "supports_null_domain_target": true
                }
            }),
            Some("{{ order }} {{ preference }} {{ flags }} {{ services }} {{ regexp }} {{ replacement }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_hinfo() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("HINFO")?,
        Some(DnsTypeCode::new(13)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Single,
            false,
            vec![
                RecordFieldSchema::new(
                    "cpu",
                    RecordFieldKind::CharString,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("os", RecordFieldKind::CharString, true, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1035"],
                    "owner_name_syntax": "hostname",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ cpu }} {{ os }}".to_string()),
        )?,
        true,
    ))
}
