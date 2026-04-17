use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::{DnsTypeCode, RecordTypeName};

pub(super) fn builtin_a() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("A")?,
        Some(DnsTypeCode::new(1)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            true,
            vec![RecordFieldSchema::new(
                "address",
                RecordFieldKind::Ipv4,
                true,
                false,
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
            Some("{{ address }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_aaaa() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("AAAA")?,
        Some(DnsTypeCode::new(28)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            true,
            vec![RecordFieldSchema::new(
                "address",
                RecordFieldKind::Ipv6,
                true,
                false,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC3596"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ address }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_ns() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("NS")?,
        Some(DnsTypeCode::new(2)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            true,
            vec![RecordFieldSchema::new(
                "nsdname",
                RecordFieldKind::Fqdn,
                true,
                false,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1035", "RFC2181"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["nsdname"],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ nsdname }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_ptr() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("PTR")?,
        Some(DnsTypeCode::new(12)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ReverseZone,
            RecordCardinality::Multiple,
            true,
            vec![RecordFieldSchema::new(
                "ptrdname",
                RecordFieldKind::Fqdn,
                true,
                false,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1035", "RFC2181"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["ptrdname"],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ ptrdname }}".to_string()),
        )?,
        true,
    ))
}
