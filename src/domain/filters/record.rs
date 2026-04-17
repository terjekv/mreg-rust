use std::collections::HashMap;

use super::apply::apply_string_filter;
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::build_sql_conditions;
use crate::domain::resource_records::{RecordInstance, RecordOwnerKind};
use crate::errors::AppError;

// ─── RecordFilter ───────────────────────────────────────────────────

/// Filter for DNS record list queries by type, owner kind, and owner name.
#[derive(Clone, Debug, Default)]
pub struct RecordFilter {
    pub type_name: Vec<FilterCondition>,
    pub owner_kind: Vec<FilterCondition>,
    pub owner_name: Vec<FilterCondition>,
}

impl RecordFilter {
    pub fn matches(&self, record: &RecordInstance) -> bool {
        for cond in &self.type_name {
            if !apply_string_filter(record.type_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.owner_kind {
            let kind_str = record
                .owner_kind()
                .map(|k| match k {
                    RecordOwnerKind::Host => "host",
                    RecordOwnerKind::ForwardZone => "forward_zone",
                    RecordOwnerKind::ForwardZoneDelegation => "forward_zone_delegation",
                    RecordOwnerKind::ReverseZone => "reverse_zone",
                    RecordOwnerKind::ReverseZoneDelegation => "reverse_zone_delegation",
                    RecordOwnerKind::NameServer => "name_server",
                })
                .unwrap_or("");
            if !apply_string_filter(kind_str, cond) {
                return false;
            }
        }
        for cond in &self.owner_name {
            if !apply_string_filter(record.owner_name(), cond) {
                return false;
            }
        }
        true
    }

    /// Generate SQL WHERE clauses for postgres.
    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        build_sql_conditions(
            &[
                (&self.type_name, "rt.name"),
                (&self.owner_kind, "rs.anchor_kind"),
                (&self.owner_name, "rs.owner_name"),
            ],
            &None,
            &[],
        )
    }

    /// Build a RecordFilter from a HashMap of query parameters.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "type_name" => {
                    validate_op("type_name", &op, FieldType::String)?;
                    filter.type_name.push(FilterCondition { op, value });
                }
                "owner_kind" => {
                    validate_op("owner_kind", &op, FieldType::Enum)?;
                    filter.owner_kind.push(FilterCondition { op, value });
                }
                "owner_name" => {
                    validate_op("owner_name", &op, FieldType::String)?;
                    filter.owner_name.push(FilterCondition { op, value });
                }
                _ => {
                    return Err(AppError::validation(format!(
                        "unknown filter field: {field}"
                    )));
                }
            }
        }
        Ok(filter)
    }
}
