use std::collections::HashMap;

use super::apply::{apply_datetime_filter, apply_string_filter, apply_u32_filter};
use super::operators::{FieldType, FilterCondition, FilterOp, parse_filter_key, validate_op};
use super::sql::build_sql_conditions;
use crate::domain::network::Network;
use crate::domain::types::IpAddressValue;
use crate::errors::AppError;

// ─── NetworkFilter ──────────────────────────────────────────────────

/// Filter for network list queries.
#[derive(Clone, Debug, Default)]
pub struct NetworkFilter {
    pub description: Vec<FilterCondition>,
    pub vlan: Vec<FilterCondition>,
    pub category: Vec<FilterCondition>,
    pub location: Vec<FilterCondition>,
    pub frozen: Vec<FilterCondition>,
    pub created_at: Vec<FilterCondition>,
    pub updated_at: Vec<FilterCondition>,
    pub family: Vec<FilterCondition>,
    // Special fields
    pub search: Option<String>,
    pub contains_ip: Option<IpAddressValue>,
}

impl NetworkFilter {
    pub fn matches(&self, network: &Network) -> bool {
        for cond in &self.description {
            if !apply_string_filter(network.description(), cond) {
                return false;
            }
        }
        for cond in &self.vlan {
            match network.vlan() {
                Some(v) => {
                    if !apply_u32_filter(v, cond) {
                        return false;
                    }
                }
                None => {
                    if cond.op != FilterOp::IsNull {
                        return false;
                    }
                }
            }
        }
        for cond in &self.category {
            if !apply_string_filter(network.category(), cond) {
                return false;
            }
        }
        for cond in &self.location {
            if !apply_string_filter(network.location(), cond) {
                return false;
            }
        }
        for cond in &self.frozen {
            let frozen_str = if network.frozen() { "true" } else { "false" };
            if !apply_string_filter(frozen_str, cond) {
                return false;
            }
        }
        for cond in &self.created_at {
            if !apply_datetime_filter(network.created_at(), cond) {
                return false;
            }
        }
        for cond in &self.updated_at {
            if !apply_datetime_filter(network.updated_at(), cond) {
                return false;
            }
        }
        for cond in &self.family {
            let family_str = match network.cidr().as_inner() {
                ipnet::IpNet::V4(_) => "4",
                ipnet::IpNet::V6(_) => "6",
            };
            if !apply_string_filter(family_str, cond) {
                return false;
            }
        }
        if let Some(ref address) = self.contains_ip
            && !network.contains(address)
        {
            return false;
        }
        if let Some(ref needle) = self.search {
            let needle = needle.to_ascii_lowercase();
            let cidr_match = network.cidr().as_str().contains(&needle);
            let desc_match = network.description().to_ascii_lowercase().contains(&needle);
            if !(cidr_match || desc_match) {
                return false;
            }
        }
        true
    }

    /// Generate SQL WHERE clauses for postgres.
    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        build_sql_conditions(
            &[
                (&self.description, "n.description"),
                (&self.vlan, "n.vlan"),
                (&self.category, "n.category"),
                (&self.location, "n.location"),
                (&self.frozen, "n.frozen::text"),
                (&self.created_at, "n.created_at"),
                (&self.updated_at, "n.updated_at"),
                (&self.family, "family(n.network)::text"),
            ],
            &self.search,
            &["n.network::text", "n.description"],
        )
    }

    /// Build a NetworkFilter from a HashMap of query parameters.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "description" => {
                    validate_op("description", &op, FieldType::String)?;
                    filter.description.push(FilterCondition { op, value });
                }
                "vlan" => {
                    validate_op("vlan", &op, FieldType::Numeric)?;
                    filter.vlan.push(FilterCondition { op, value });
                }
                "category" => {
                    validate_op("category", &op, FieldType::String)?;
                    filter.category.push(FilterCondition { op, value });
                }
                "location" => {
                    validate_op("location", &op, FieldType::String)?;
                    filter.location.push(FilterCondition { op, value });
                }
                "frozen" => {
                    validate_op("frozen", &op, FieldType::Enum)?;
                    filter.frozen.push(FilterCondition { op, value });
                }
                "created_at" => {
                    validate_op("created_at", &op, FieldType::DateTime)?;
                    filter.created_at.push(FilterCondition { op, value });
                }
                "updated_at" => {
                    validate_op("updated_at", &op, FieldType::DateTime)?;
                    filter.updated_at.push(FilterCondition { op, value });
                }
                "family" => {
                    validate_op("family", &op, FieldType::Enum)?;
                    filter.family.push(FilterCondition { op, value });
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
