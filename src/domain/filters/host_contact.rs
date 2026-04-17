use std::collections::HashMap;

use super::apply::{apply_datetime_filter, apply_optional_string_filter, apply_string_filter};
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::{build_sql_conditions, op_to_sql};
use crate::domain::host_contact::HostContact;
use crate::errors::AppError;

// ─── HostContactFilter ─────────────────────────────────────────────

/// Filter for host contact list queries.
#[derive(Clone, Debug, Default)]
pub struct HostContactFilter {
    pub email: Vec<FilterCondition>,
    pub display_name: Vec<FilterCondition>,
    pub created_at: Vec<FilterCondition>,
    pub updated_at: Vec<FilterCondition>,
    pub host: Vec<FilterCondition>,
    // Special fields
    pub search: Option<String>,
}

impl HostContactFilter {
    pub fn matches(&self, contact: &HostContact) -> bool {
        for cond in &self.email {
            if !apply_string_filter(contact.email().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.display_name {
            if !apply_optional_string_filter(contact.display_name(), cond) {
                return false;
            }
        }
        for cond in &self.created_at {
            if !apply_datetime_filter(contact.created_at(), cond) {
                return false;
            }
        }
        for cond in &self.updated_at {
            if !apply_datetime_filter(contact.updated_at(), cond) {
                return false;
            }
        }
        for cond in &self.host {
            if !contact
                .hosts()
                .iter()
                .any(|h| apply_string_filter(h.as_str(), cond))
            {
                return false;
            }
        }
        if let Some(ref needle) = self.search {
            let needle = needle.to_ascii_lowercase();
            let email_match = contact.email().as_str().contains(&needle);
            let name_match = contact
                .display_name()
                .is_some_and(|n| n.to_ascii_lowercase().contains(&needle));
            if !(email_match || name_match) {
                return false;
            }
        }
        true
    }

    /// Generate SQL WHERE clauses for postgres.
    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        let (mut clauses, mut values) = build_sql_conditions(
            &[
                (&self.email, "hc.email::text"),
                (&self.display_name, "hc.display_name"),
                (&self.created_at, "hc.created_at"),
                (&self.updated_at, "hc.updated_at"),
            ],
            &self.search,
            &["hc.email::text", "hc.display_name"],
        );

        // Host conditions use an EXISTS subquery, so handle separately.
        let mut idx = values.len() + 1;
        for cond in &self.host {
            let (sql, val, consumed) = op_to_sql(&cond.op, "h.name::text", &cond.value, idx);
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM host_contacts_hosts hch \
                 JOIN hosts h ON h.id = hch.host_id \
                 WHERE hch.contact_id = hc.id AND {sql})"
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
                "email" => {
                    validate_op("email", &op, FieldType::String)?;
                    filter.email.push(FilterCondition { op, value });
                }
                "display_name" => {
                    validate_op("display_name", &op, FieldType::String)?;
                    filter.display_name.push(FilterCondition { op, value });
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
