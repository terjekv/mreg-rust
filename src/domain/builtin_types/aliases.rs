use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::RecordTypeName;

pub(super) fn builtin_cname() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("CNAME")?,
        Some(5),
        RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Single,
            false,
            vec![RecordFieldSchema::new(
                "target",
                RecordFieldKind::Fqdn,
                true,
                false,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1034", "RFC1035", "RFC2181"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": true,
                    "blocks_other_types_when_present": true,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ target }}".to_string()),
        )?,
        true,
    ))
}

/// DNAME (RFC 6672) — delegation name, like CNAME but for entire subtrees.
pub(super) fn builtin_dname() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("DNAME")?,
        Some(39),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Single,
            true,
            vec![RecordFieldSchema::new(
                "target",
                RecordFieldKind::Fqdn,
                true,
                false,
                Vec::new(),
            )?],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC6672"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": true,
                    "blocks_other_types_when_present": true,
                    "target_fields_must_not_be_aliases": [],
                    "supports_null_domain_target": false
                }
            }),
            Some("{{ target }}".to_string()),
        )?,
        true,
    ))
}
