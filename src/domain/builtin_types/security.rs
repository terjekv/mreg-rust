use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::{DnsTypeCode, RecordTypeName};

pub(super) fn builtin_sshfp() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("SSHFP")?,
        Some(DnsTypeCode::new(44)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new(
                    "algorithm",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "fp_type",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "fingerprint",
                    RecordFieldKind::Hex,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC4255", "RFC6594", "RFC7479"],
                    "owner_name_syntax": "hostname",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ algorithm }} {{ fp_type }} {{ fingerprint }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_ds() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("DS")?,
        Some(DnsTypeCode::new(43)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            true,
            vec![
                RecordFieldSchema::new(
                    "key_tag",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "algorithm",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "digest_type",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("digest", RecordFieldKind::Hex, true, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC4034", "RFC4509"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ key_tag }} {{ algorithm }} {{ digest_type }} {{ digest }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_dnskey() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("DNSKEY")?,
        Some(DnsTypeCode::new(48)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            true,
            vec![
                RecordFieldSchema::new("flags", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new(
                    "protocol",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "algorithm",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "public_key",
                    RecordFieldKind::Text,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC4034"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ flags }} {{ protocol }} {{ algorithm }} {{ public_key }}".to_string()),
        )?,
        true,
    ))
}

/// CDS (RFC 7344) — child DS, signals DS changes to the parent zone.
pub(super) fn builtin_cds() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("CDS")?,
        Some(DnsTypeCode::new(59)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            true,
            vec![
                RecordFieldSchema::new(
                    "key_tag",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "algorithm",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "digest_type",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("digest", RecordFieldKind::Hex, true, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC7344", "RFC8078"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ key_tag }} {{ algorithm }} {{ digest_type }} {{ digest }}".to_string()),
        )?,
        true,
    ))
}

/// CDNSKEY (RFC 7344) — child DNSKEY, signals DNSKEY changes to the parent zone.
pub(super) fn builtin_cdnskey() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("CDNSKEY")?,
        Some(DnsTypeCode::new(60)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            true,
            vec![
                RecordFieldSchema::new("flags", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new(
                    "protocol",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "algorithm",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "public_key",
                    RecordFieldKind::Text,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC7344", "RFC8078"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ flags }} {{ protocol }} {{ algorithm }} {{ public_key }}".to_string()),
        )?,
        true,
    ))
}

/// CSYNC (RFC 7477) — child-to-parent synchronization.
pub(super) fn builtin_csync() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("CSYNC")?,
        Some(DnsTypeCode::new(62)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Single,
            true,
            vec![
                RecordFieldSchema::new(
                    "soa_serial",
                    RecordFieldKind::Uint32,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new("flags", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new(
                    "type_bitmap",
                    RecordFieldKind::Text,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC7477"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ soa_serial }} {{ flags }} {{ type_bitmap }}".to_string()),
        )?,
        true,
    ))
}

pub(super) fn builtin_caa() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("CAA")?,
        Some(DnsTypeCode::new(257)?),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new("flags", RecordFieldKind::Uint16, true, false, Vec::new())?,
                RecordFieldSchema::new("tag", RecordFieldKind::Text, true, false, Vec::new())?,
                RecordFieldSchema::new("value", RecordFieldKind::Text, true, false, Vec::new())?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC8659"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ flags }} {{ tag }} \"{{ value }}\"".to_string()),
        )?,
        true,
    ))
}
