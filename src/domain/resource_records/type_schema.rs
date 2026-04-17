use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    domain::record_validation::{allows_raw_rdata_from_flags, validate_field_value},
    errors::AppError,
};

use super::{RecordCardinality, RecordFieldSchema, RecordOwnerKind, RecordOwnerNameSyntax};

/// RFC-backed behavioral constraints for a record type (TTL matching, exclusivity, alias targets).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
pub struct RecordRfcProfile {
    rfc_refs: Vec<String>,
    owner_name_syntax: RecordOwnerNameSyntax,
    rrset_ttl_must_match: bool,
    exclusive_with_other_types_at_owner: bool,
    blocks_other_types_when_present: bool,
    target_fields_must_not_be_aliases: Vec<String>,
    supports_null_domain_target: bool,
}

impl RecordRfcProfile {
    pub fn new(
        rfc_refs: Vec<String>,
        owner_name_syntax: RecordOwnerNameSyntax,
        rrset_ttl_must_match: bool,
        exclusive_with_other_types_at_owner: bool,
        blocks_other_types_when_present: bool,
        target_fields_must_not_be_aliases: Vec<String>,
        supports_null_domain_target: bool,
    ) -> Self {
        Self {
            rfc_refs,
            owner_name_syntax,
            rrset_ttl_must_match,
            exclusive_with_other_types_at_owner,
            blocks_other_types_when_present,
            target_fields_must_not_be_aliases,
            supports_null_domain_target,
        }
    }

    pub fn rfc_refs(&self) -> &[String] {
        &self.rfc_refs
    }

    pub fn owner_name_syntax(&self) -> &RecordOwnerNameSyntax {
        &self.owner_name_syntax
    }

    pub fn rrset_ttl_must_match(&self) -> bool {
        self.rrset_ttl_must_match
    }

    pub fn exclusive_with_other_types_at_owner(&self) -> bool {
        self.exclusive_with_other_types_at_owner
    }

    pub fn blocks_other_types_when_present(&self) -> bool {
        self.blocks_other_types_when_present
    }

    pub fn target_fields_must_not_be_aliases(&self) -> &[String] {
        &self.target_fields_must_not_be_aliases
    }

    pub fn supports_null_domain_target(&self) -> bool {
        self.supports_null_domain_target
    }
}

/// Validation and rendering schema for a record type, including fields and behavior flags.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecordTypeSchema {
    owner_kind: RecordOwnerKind,
    cardinality: RecordCardinality,
    zone_bound: bool,
    fields: Vec<RecordFieldSchema>,
    behavior_flags: Value,
    render_template: Option<String>,
}

impl RecordTypeSchema {
    pub fn new(
        owner_kind: RecordOwnerKind,
        cardinality: RecordCardinality,
        zone_bound: bool,
        fields: Vec<RecordFieldSchema>,
        behavior_flags: Value,
        render_template: Option<String>,
    ) -> Result<Self, AppError> {
        if fields.is_empty() && !allows_raw_rdata_from_flags(&behavior_flags) {
            return Err(AppError::validation(
                "record type schema must define at least one field unless RFC 3597 raw RDATA support is enabled",
            ));
        }

        Ok(Self {
            owner_kind,
            cardinality,
            zone_bound,
            fields,
            behavior_flags,
            render_template,
        })
    }

    pub fn owner_kind(&self) -> &RecordOwnerKind {
        &self.owner_kind
    }

    pub fn cardinality(&self) -> &RecordCardinality {
        &self.cardinality
    }

    pub fn zone_bound(&self) -> bool {
        self.zone_bound
    }

    pub fn fields(&self) -> &[RecordFieldSchema] {
        &self.fields
    }

    pub fn behavior_flags(&self) -> &Value {
        &self.behavior_flags
    }

    pub fn render_template(&self) -> Option<&str> {
        self.render_template.as_deref()
    }

    pub fn rfc_profile(&self) -> Result<Option<RecordRfcProfile>, AppError> {
        self.behavior_flags
            .get("rfc_profile")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(AppError::internal)
    }

    pub fn allows_raw_rdata(&self) -> bool {
        allows_raw_rdata_from_flags(&self.behavior_flags)
    }

    pub fn validate_and_normalize(&self, payload: &Value) -> Result<Value, AppError> {
        let object = payload
            .as_object()
            .ok_or_else(|| AppError::validation("record payload must be a JSON object"))?;
        let mut normalized = Map::new();

        for field in &self.fields {
            match object.get(field.name()) {
                Some(value) => {
                    let normalized_value = if field.repeated() {
                        let items = if let Some(array) = value.as_array() {
                            array.clone()
                        } else {
                            vec![value.clone()]
                        };
                        let mut normalized_items = Vec::with_capacity(items.len());
                        for item in &items {
                            normalized_items.push(validate_field_value(field, item)?);
                        }
                        Value::Array(normalized_items)
                    } else {
                        validate_field_value(field, value)?
                    };
                    normalized.insert(field.name().to_string(), normalized_value);
                }
                None if field.required() => {
                    return Err(AppError::validation(format!(
                        "record field '{}' is required",
                        field.name()
                    )));
                }
                None => {}
            }
        }

        Ok(Value::Object(normalized))
    }
}
