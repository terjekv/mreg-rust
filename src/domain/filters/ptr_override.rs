use std::collections::HashMap;

use super::apply::apply_string_filter;
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use crate::domain::ptr_override::PtrOverride;
use crate::errors::AppError;

// ─── PtrOverrideFilter ──────────────────────────────────────────────

/// Filter for PTR override queries.
#[derive(Clone, Debug, Default)]
pub struct PtrOverrideFilter {
    pub host: Vec<FilterCondition>,
    pub address: Vec<FilterCondition>,
}

impl PtrOverrideFilter {
    pub fn matches(&self, ptr: &PtrOverride) -> bool {
        for cond in &self.host {
            if !apply_string_filter(ptr.host_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.address {
            if !apply_string_filter(&ptr.address().as_str(), cond) {
                return false;
            }
        }
        true
    }

    /// Build from query params.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "host" => {
                    validate_op("host", &op, FieldType::String)?;
                    filter.host.push(FilterCondition { op, value });
                }
                "address" => {
                    validate_op("address", &op, FieldType::String)?;
                    filter.address.push(FilterCondition { op, value });
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
