use serde_json::json;

use crate::{
    domain::resource_records::{
        CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema,
    },
    errors::AppError,
};

use crate::domain::types::{DnsTypeCode, RecordTypeName};

pub(super) fn builtin_mx() -> Result<CreateRecordTypeDefinition, AppError> {
    Ok(CreateRecordTypeDefinition::new(
        RecordTypeName::new("MX")?,
        Some(DnsTypeCode::new(15)?),
        RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            true,
            vec![
                RecordFieldSchema::new(
                    "preference",
                    RecordFieldKind::Uint16,
                    true,
                    false,
                    Vec::new(),
                )?,
                RecordFieldSchema::new(
                    "exchange",
                    RecordFieldKind::DomainName,
                    true,
                    false,
                    Vec::new(),
                )?,
            ],
            json!({
                "rfc_profile": {
                    "rfc_refs": ["RFC1035", "RFC2181", "RFC7505"],
                    "owner_name_syntax": "dns_name",
                    "rrset_ttl_must_match": true,
                    "exclusive_with_other_types_at_owner": false,
                    "blocks_other_types_when_present": false,
                    "target_fields_must_not_be_aliases": ["exchange"],
                    "supports_null_domain_target": true
                }
            }),
            Some("{{ preference }} {{ exchange }}".to_string()),
        )?,
        true,
    ))
}
