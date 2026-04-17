use std::collections::HashMap;

use super::apply::{apply_string_filter, apply_u32_filter};
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::build_sql_conditions;
use crate::domain::bacnet::BacnetIdAssignment;
use crate::errors::AppError;

// ─── BacnetIdFilter ─────────────────────────────────────────────────

/// Filter for BACnet ID assignment queries.
#[derive(Clone, Debug, Default)]
pub struct BacnetIdFilter {
    pub bacnet_id: Vec<FilterCondition>,
    pub host: Vec<FilterCondition>,
}

impl BacnetIdFilter {
    pub fn matches(&self, assignment: &BacnetIdAssignment) -> bool {
        for cond in &self.bacnet_id {
            if !apply_u32_filter(assignment.bacnet_id().as_u32(), cond) {
                return false;
            }
        }
        for cond in &self.host {
            if !apply_string_filter(assignment.host_name().as_str(), cond) {
                return false;
            }
        }
        true
    }

    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        build_sql_conditions(
            &[(&self.bacnet_id, "b.id"), (&self.host, "h.name::text")],
            &None,
            &[],
        )
    }

    /// Build from query params.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "bacnet_id" => {
                    validate_op("bacnet_id", &op, FieldType::Numeric)?;
                    filter.bacnet_id.push(FilterCondition { op, value });
                }
                "host" => {
                    validate_op("host", &op, FieldType::String)?;
                    filter.host.push(FilterCondition { op, value });
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
