use std::collections::{BTreeMap, HashMap};

use super::apply::{apply_datetime_filter, apply_optional_string_filter, apply_string_filter};
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::{build_sql_conditions, op_to_sql};
use crate::domain::host::{Host, IpAddressAssignment};
use crate::errors::AppError;

// ─── HostFilter ─────────────────────────────────────────────────────

/// Filter for host list queries.
///
/// Operator-based fields use `Vec<FilterCondition>` and combine with AND logic.
/// Special fields (`search`) are handled separately.
#[derive(Clone, Debug, Default)]
pub struct HostFilter {
    pub name: Vec<FilterCondition>,
    pub zone: Vec<FilterCondition>,
    pub comment: Vec<FilterCondition>,
    pub created_at: Vec<FilterCondition>,
    pub updated_at: Vec<FilterCondition>,
    pub address: Vec<FilterCondition>,
    // Special fields (not operator-based)
    pub search: Option<String>,
}

impl HostFilter {
    pub fn matches(
        &self,
        host: &Host,
        ip_addresses: &BTreeMap<String, IpAddressAssignment>,
    ) -> bool {
        for cond in &self.name {
            if !apply_string_filter(host.name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.zone {
            if !apply_optional_string_filter(host.zone().map(|z| z.as_str()), cond) {
                return false;
            }
        }
        for cond in &self.comment {
            if !apply_string_filter(host.comment(), cond) {
                return false;
            }
        }
        for cond in &self.created_at {
            if !apply_datetime_filter(host.created_at(), cond) {
                return false;
            }
        }
        for cond in &self.updated_at {
            if !apply_datetime_filter(host.updated_at(), cond) {
                return false;
            }
        }
        for cond in &self.address {
            let host_ips: Vec<String> = ip_addresses
                .values()
                .filter(|a| a.host_id() == host.id())
                .map(|a| a.address().as_str().to_string())
                .collect();
            if !host_ips.iter().any(|ip| apply_string_filter(ip, cond)) {
                return false;
            }
        }
        if let Some(ref needle) = self.search {
            let needle = needle.to_ascii_lowercase();
            let name_match = host.name().as_str().to_ascii_lowercase().contains(&needle);
            let zone_match = host
                .zone()
                .is_some_and(|z| z.as_str().to_ascii_lowercase().contains(&needle));
            let comment_match = host.comment().to_ascii_lowercase().contains(&needle);
            if !(name_match || zone_match || comment_match) {
                return false;
            }
        }
        true
    }

    /// Generate SQL WHERE clauses for postgres. Returns (clauses, bind_values).
    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        let (mut clauses, mut values) = build_sql_conditions(
            &[
                (&self.name, "h.name::text"),
                (&self.zone, "fz.name::text"),
                (&self.comment, "h.comment"),
                (&self.created_at, "h.created_at"),
                (&self.updated_at, "h.updated_at"),
            ],
            &self.search,
            &["h.name::text", "fz.name::text", "h.comment"],
        );

        // Address conditions use an EXISTS subquery, so handle separately.
        let mut idx = values.len() + 1;
        for cond in &self.address {
            let (sql, val, consumed) =
                op_to_sql(&cond.op, "host(ia.address)::text", &cond.value, idx);
            clauses.push(format!(
                "EXISTS (SELECT 1 FROM ip_addresses ia WHERE ia.host_id = h.id AND {sql})"
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

    /// Build a HostFilter from a HashMap of query parameters.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "name" => {
                    validate_op("name", &op, FieldType::String)?;
                    filter.name.push(FilterCondition { op, value });
                }
                "zone" => {
                    validate_op("zone", &op, FieldType::String)?;
                    filter.zone.push(FilterCondition { op, value });
                }
                "comment" => {
                    validate_op("comment", &op, FieldType::String)?;
                    filter.comment.push(FilterCondition { op, value });
                }
                "created_at" => {
                    validate_op("created_at", &op, FieldType::DateTime)?;
                    filter.created_at.push(FilterCondition { op, value });
                }
                "updated_at" => {
                    validate_op("updated_at", &op, FieldType::DateTime)?;
                    filter.updated_at.push(FilterCondition { op, value });
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
