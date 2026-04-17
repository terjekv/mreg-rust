mod field_schema;
mod instances;
mod ownership;
mod raw_rdata;
mod type_definition;
mod type_schema;

pub use field_schema::*;
pub use instances::*;
pub use ownership::*;
pub use raw_rdata::*;
pub use type_definition::*;
pub use type_schema::*;

// Re-export public functions from extracted modules so existing callers continue to work.
pub use crate::domain::builtin_types::built_in_record_types;
pub use crate::domain::record_validation::{alias_target_names, validate_record_relationships};

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ExistingRecordSummary, RecordCardinality, RecordFieldKind, RecordFieldSchema,
        RecordOwnerKind, RecordTypeSchema, ValidatedRecordContent, built_in_record_types,
        validate_record_relationships,
    };
    use crate::domain::types::{DnsName, RecordTypeName, Ttl};

    #[test]
    fn record_schema_normalizes_fqdn_fields() {
        let schema = RecordTypeSchema::new(
            RecordOwnerKind::Host,
            RecordCardinality::Single,
            false,
            vec![
                RecordFieldSchema::new("target", RecordFieldKind::Fqdn, true, false, Vec::new())
                    .expect("schema field"),
            ],
            json!({}),
            None,
        )
        .expect("schema");

        let normalized = schema
            .validate_and_normalize(&json!({"target":"NS1.Example.Org."}))
            .expect("payload should validate");
        assert_eq!(normalized["target"], "ns1.example.org");
    }

    #[test]
    fn domain_name_fields_allow_root() {
        let schema = RecordTypeSchema::new(
            RecordOwnerKind::ForwardZone,
            RecordCardinality::Multiple,
            false,
            vec![
                RecordFieldSchema::new(
                    "exchange",
                    RecordFieldKind::DomainName,
                    true,
                    false,
                    Vec::new(),
                )
                .expect("schema field"),
            ],
            json!({}),
            None,
        )
        .expect("schema");

        let normalized = schema
            .validate_and_normalize(&json!({"exchange":"."}))
            .expect("root domain target should validate");
        assert_eq!(normalized["exchange"], ".");
    }

    #[test]
    fn txt_payload_is_normalized_to_character_string_array() {
        let txt = built_in_record_types()
            .expect("builtins")
            .into_iter()
            .find(|item| item.name().as_str() == "TXT")
            .expect("TXT definition");
        let definition = super::RecordTypeDefinition::restore(
            uuid::Uuid::new_v4(),
            txt.name().clone(),
            txt.dns_type(),
            txt.schema().clone(),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );

        let normalized = definition
            .validate_record_input(
                &DnsName::new("txt.example.org").expect("owner"),
                Some(&json!({"value":"hello"})),
                None,
            )
            .expect("TXT payload should validate");
        let ValidatedRecordContent::Structured(normalized) = normalized else {
            panic!("expected structured data");
        };
        assert_eq!(normalized["value"], json!(["hello"]));
    }

    #[test]
    fn naptr_requires_exactly_one_of_regexp_or_replacement() {
        let naptr = built_in_record_types()
            .expect("builtins")
            .into_iter()
            .find(|item| item.name().as_str() == "NAPTR")
            .expect("NAPTR definition");
        let definition = super::RecordTypeDefinition::restore(
            uuid::Uuid::new_v4(),
            naptr.name().clone(),
            naptr.dns_type(),
            naptr.schema().clone(),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );

        let error = definition
            .validate_record_input(
                &DnsName::new("naptr.example.org").expect("owner"),
                Some(&json!({
                    "order": 100,
                    "preference": 10,
                    "flags": "s",
                    "services": "SIP+D2U",
                    "regexp": "!^.*$!sip:info@example.org!",
                    "replacement": "replacement.example.org"
                })),
                None,
            )
            .expect_err("NAPTR should reject both regexp and replacement");
        assert!(error.to_string().contains("exactly one"));
    }

    #[test]
    fn sshfp_rejects_unknown_algorithm() {
        let sshfp = built_in_record_types()
            .expect("builtins")
            .into_iter()
            .find(|item| item.name().as_str() == "SSHFP")
            .expect("SSHFP definition");
        let definition = super::RecordTypeDefinition::restore(
            uuid::Uuid::new_v4(),
            sshfp.name().clone(),
            sshfp.dns_type(),
            sshfp.schema().clone(),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );

        assert!(
            definition
                .validate_record_input(
                    &DnsName::new("ssh.example.org").expect("owner"),
                    Some(&json!({
                        "algorithm": 99,
                        "fp_type": 2,
                        "fingerprint": "abcdef0123456789"
                    })),
                    None,
                )
                .is_err()
        );
    }

    #[test]
    fn rrset_ttl_must_match_existing_records() {
        let cname = built_in_record_types()
            .expect("builtins")
            .into_iter()
            .find(|item| item.name().as_str() == "MX")
            .expect("MX definition");
        let definition = super::RecordTypeDefinition::restore(
            uuid::Uuid::new_v4(),
            cname.name().clone(),
            cname.dns_type(),
            cname.schema().clone(),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );

        let error = validate_record_relationships(
            &definition,
            Some(Ttl::new(600).expect("ttl")),
            &ValidatedRecordContent::Structured(
                json!({"preference": 10, "exchange": "mail.example.org"}),
            ),
            &[],
            &[ExistingRecordSummary::new(
                RecordTypeName::new("MX").expect("name"),
                Some(Ttl::new(300).expect("ttl")),
                json!({"preference": 20, "exchange": "backup.example.org"}),
                None,
            )],
            &std::collections::BTreeSet::new(),
        )
        .expect_err("TTL mismatch should fail");

        assert!(error.to_string().contains("RRSet"));
    }

    #[test]
    fn cname_cannot_coexist_with_other_data() {
        let cname = built_in_record_types()
            .expect("builtins")
            .into_iter()
            .find(|item| item.name().as_str() == "CNAME")
            .expect("CNAME definition");
        let definition = super::RecordTypeDefinition::restore(
            uuid::Uuid::new_v4(),
            cname.name().clone(),
            cname.dns_type(),
            cname.schema().clone(),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );

        let error = validate_record_relationships(
            &definition,
            Some(Ttl::new(300).expect("ttl")),
            &ValidatedRecordContent::Structured(json!({"target":"alias.example.org"})),
            &[ExistingRecordSummary::new(
                RecordTypeName::new("TXT").expect("name"),
                Some(Ttl::new(300).expect("ttl")),
                json!({"value":["hello"]}),
                None,
            )],
            &[],
            &std::collections::BTreeSet::new(),
        )
        .expect_err("CNAME should not coexist");
        assert!(error.to_string().contains("cannot coexist"));
    }

    #[test]
    fn null_mx_cannot_coexist_with_other_mx_records() {
        let mx = built_in_record_types()
            .expect("builtins")
            .into_iter()
            .find(|item| item.name().as_str() == "MX")
            .expect("MX definition");
        let definition = super::RecordTypeDefinition::restore(
            uuid::Uuid::new_v4(),
            mx.name().clone(),
            mx.dns_type(),
            mx.schema().clone(),
            true,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );

        let error = validate_record_relationships(
            &definition,
            Some(Ttl::new(300).expect("ttl")),
            &ValidatedRecordContent::Structured(json!({"preference": 0, "exchange": "."})),
            &[],
            &[ExistingRecordSummary::new(
                RecordTypeName::new("MX").expect("name"),
                Some(Ttl::new(300).expect("ttl")),
                json!({"preference": 20, "exchange": "mail.example.org"}),
                None,
            )],
            &std::collections::BTreeSet::new(),
        )
        .expect_err("null MX should be exclusive");
        assert!(error.to_string().contains("null MX"));
    }
}
