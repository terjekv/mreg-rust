use std::collections::HashMap;

use super::apply::{apply_datetime_filter, apply_string_filter};
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::{build_sql_conditions, op_to_sql};
use crate::domain::host_group::HostGroup;
use crate::errors::AppError;

// ─── HostGroupFilter ────────────────────────────────────────────────

/// Filter for host group list queries.
#[derive(Clone, Debug, Default)]
pub struct HostGroupFilter {
    pub name: Vec<FilterCondition>,
    pub description: Vec<FilterCondition>,
    pub created_at: Vec<FilterCondition>,
    pub updated_at: Vec<FilterCondition>,
    pub host: Vec<FilterCondition>,
    // Special fields
    pub search: Option<String>,
}

impl HostGroupFilter {
    pub fn matches(&self, group: &HostGroup) -> bool {
        for cond in &self.name {
            if !apply_string_filter(group.name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.description {
            if !apply_string_filter(group.description(), cond) {
                return false;
            }
        }
        for cond in &self.created_at {
            if !apply_datetime_filter(group.created_at(), cond) {
                return false;
            }
        }
        for cond in &self.updated_at {
            if !apply_datetime_filter(group.updated_at(), cond) {
                return false;
            }
        }
        for cond in &self.host {
            if !group
                .hosts()
                .iter()
                .any(|h| apply_string_filter(h.as_str(), cond))
            {
                return false;
            }
        }
        if let Some(ref needle) = self.search {
            let needle = needle.to_ascii_lowercase();
            let name_match = group.name().as_str().contains(&needle);
            let desc_match = group.description().to_ascii_lowercase().contains(&needle);
            if !(name_match || desc_match) {
                return false;
            }
        }
        true
    }

    /// Generate SQL WHERE clauses for postgres.
    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        let (mut clauses, mut values) = build_sql_conditions(
            &[
                (&self.name, "hg.name::text"),
                (&self.description, "hg.description"),
                (&self.created_at, "hg.created_at"),
                (&self.updated_at, "hg.updated_at"),
            ],
            &self.search,
            &["hg.name::text", "hg.description"],
        );

        // Host conditions use an EXISTS subquery, so handle separately.
        let mut idx = values.len() + 1;
        for cond in &self.host {
            let (sql, val, consumed) = op_to_sql(&cond.op, "h.name::text", &cond.value, idx);
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM host_group_hosts hgh \
                 JOIN hosts h ON h.id = hgh.host_id \
                 WHERE hgh.host_group_id = hg.id AND {sql})"
            ));
            if let Some(v) = val {
                values.push(v);
            }
            if consumed {
                idx += 1;
            }
        }

        (clauses, values)
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
