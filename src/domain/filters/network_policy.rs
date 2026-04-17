use std::collections::HashMap;

use super::apply::{apply_datetime_filter, apply_string_filter};
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::build_sql_conditions;
use crate::domain::network_policy::NetworkPolicy;
use crate::errors::AppError;

// ─── NetworkPolicyFilter ────────────────────────────────────────────

/// Filter for network policy queries.
#[derive(Clone, Debug, Default)]
pub struct NetworkPolicyFilter {
    pub name: Vec<FilterCondition>,
    pub description: Vec<FilterCondition>,
    pub created_at: Vec<FilterCondition>,
    pub updated_at: Vec<FilterCondition>,
    // Special fields
    pub search: Option<String>,
}

impl NetworkPolicyFilter {
    pub fn matches(&self, policy: &NetworkPolicy) -> bool {
        for cond in &self.name {
            if !apply_string_filter(policy.name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.description {
            if !apply_string_filter(policy.description(), cond) {
                return false;
            }
        }
        for cond in &self.created_at {
            if !apply_datetime_filter(policy.created_at(), cond) {
                return false;
            }
        }
        for cond in &self.updated_at {
            if !apply_datetime_filter(policy.updated_at(), cond) {
                return false;
            }
        }
        if let Some(ref needle) = self.search {
            let needle = needle.to_ascii_lowercase();
            let name_match = policy.name().as_str().contains(&needle);
            let desc_match = policy.description().to_ascii_lowercase().contains(&needle);
            if !(name_match || desc_match) {
                return false;
            }
        }
        true
    }

    /// Generate SQL WHERE clauses for postgres.
    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        build_sql_conditions(
            &[
                (&self.name, "np.name::text"),
                (&self.description, "np.description"),
                (&self.created_at, "np.created_at"),
                (&self.updated_at, "np.updated_at"),
            ],
            &self.search,
            &["np.name::text", "np.description"],
        )
    }

    /// Build from query params.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "name" => {
                    validate_op("name", &op, FieldType::String)?;
                    filter.name.push(FilterCondition { op, value });
                }
                "description" => {
                    validate_op("description", &op, FieldType::String)?;
                    filter.description.push(FilterCondition { op, value });
                }
                "created_at" => {
                    validate_op("created_at", &op, FieldType::DateTime)?;
                    filter.created_at.push(FilterCondition { op, value });
                }
                "updated_at" => {
                    validate_op("updated_at", &op, FieldType::DateTime)?;
                    filter.updated_at.push(FilterCondition { op, value });
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
