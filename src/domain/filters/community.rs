use std::collections::HashMap;

use super::apply::apply_string_filter;
use super::operators::{FieldType, FilterCondition, parse_filter_key, validate_op};
use super::sql::{build_sql_conditions, op_to_sql};
use crate::domain::{
    attachment::AttachmentCommunityAssignment, community::Community,
    host_community_assignment::HostCommunityAssignment,
};
use crate::errors::AppError;

// ─── CommunityFilter ───────────────────────────────────────────────

/// Filter for community queries.
#[derive(Clone, Debug, Default)]
pub struct CommunityFilter {
    pub policy_name: Vec<FilterCondition>,
    pub name: Vec<FilterCondition>,
    pub description: Vec<FilterCondition>,
    pub network: Vec<FilterCondition>,
    // Special fields
    pub search: Option<String>,
}

/// Filter for attachment-to-community assignment queries.
#[derive(Clone, Debug, Default)]
pub struct AttachmentCommunityAssignmentFilter {
    pub community_name: Vec<FilterCondition>,
    pub policy_name: Vec<FilterCondition>,
    pub host: Vec<FilterCondition>,
    pub network: Vec<FilterCondition>,
    pub mac_address: Vec<FilterCondition>,
}

impl AttachmentCommunityAssignmentFilter {
    pub fn matches(&self, assignment: &AttachmentCommunityAssignment) -> bool {
        for cond in &self.community_name {
            if !apply_string_filter(assignment.community_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.policy_name {
            if !apply_string_filter(assignment.policy_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.host {
            if !apply_string_filter(assignment.host_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.network {
            if !apply_string_filter(&assignment.network_cidr().as_str(), cond) {
                return false;
            }
        }
        let _ = &self.mac_address;
        true
    }

    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        build_sql_conditions(
            &[
                (&self.community_name, "c.name::text"),
                (&self.policy_name, "np.name::text"),
                (&self.host, "h.name::text"),
                (&self.network, "n.network::text"),
                (&self.mac_address, "a.mac_address"),
            ],
            &None,
            &[],
        )
    }

    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "community_name" => {
                    validate_op("community_name", &op, FieldType::String)?;
                    filter.community_name.push(FilterCondition { op, value });
                }
                "policy_name" => {
                    validate_op("policy_name", &op, FieldType::String)?;
                    filter.policy_name.push(FilterCondition { op, value });
                }
                "host" => {
                    validate_op("host", &op, FieldType::String)?;
                    filter.host.push(FilterCondition { op, value });
                }
                "network" => {
                    validate_op("network", &op, FieldType::Cidr)?;
                    filter.network.push(FilterCondition { op, value });
                }
                "mac_address" => {
                    validate_op("mac_address", &op, FieldType::String)?;
                    filter.mac_address.push(FilterCondition { op, value });
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

impl CommunityFilter {
    pub fn matches(&self, community: &Community) -> bool {
        for cond in &self.policy_name {
            if !apply_string_filter(community.policy_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.name {
            if !apply_string_filter(community.name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.description {
            if !apply_string_filter(community.description(), cond) {
                return false;
            }
        }
        for cond in &self.network {
            if !apply_string_filter(&community.network_cidr().as_str(), cond) {
                return false;
            }
        }
        if let Some(ref needle) = self.search {
            let needle = needle.to_ascii_lowercase();
            let name_match = community.name().as_str().contains(&needle);
            let desc_match = community
                .description()
                .to_ascii_lowercase()
                .contains(&needle);
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
                (&self.policy_name, "np.name::text"),
                (&self.name, "c.name::text"),
                (&self.description, "c.description"),
                (&self.network, "n.network::text"),
            ],
            &self.search,
            &["c.name::text", "c.description"],
        )
    }

    /// Build from query params.
    pub fn from_query_params(params: HashMap<String, String>) -> Result<Self, AppError> {
        let mut filter = Self::default();
        for (key, value) in params {
            let (field, op) = parse_filter_key(&key)?;
            match field.as_str() {
                "policy_name" => {
                    validate_op("policy_name", &op, FieldType::String)?;
                    filter.policy_name.push(FilterCondition { op, value });
                }
                "name" => {
                    validate_op("name", &op, FieldType::String)?;
                    filter.name.push(FilterCondition { op, value });
                }
                "description" => {
                    validate_op("description", &op, FieldType::String)?;
                    filter.description.push(FilterCondition { op, value });
                }
                "network" => {
                    validate_op("network", &op, FieldType::Cidr)?;
                    filter.network.push(FilterCondition { op, value });
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

// ─── HostCommunityAssignmentFilter ─────────────────────────────────────

/// Filter for host-to-community mapping queries.
#[derive(Clone, Debug, Default)]
pub struct HostCommunityAssignmentFilter {
    pub community_name: Vec<FilterCondition>,
    pub policy_name: Vec<FilterCondition>,
    pub host: Vec<FilterCondition>,
    pub address: Vec<FilterCondition>,
}

impl HostCommunityAssignmentFilter {
    pub fn matches(&self, mapping: &HostCommunityAssignment) -> bool {
        for cond in &self.community_name {
            if !apply_string_filter(mapping.community_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.policy_name {
            if !apply_string_filter(mapping.policy_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.host {
            if !apply_string_filter(mapping.host_name().as_str(), cond) {
                return false;
            }
        }
        for cond in &self.address {
            if !apply_string_filter(&mapping.address().as_str(), cond) {
                return false;
            }
        }
        true
    }

    pub fn sql_conditions(&self) -> (Vec<String>, Vec<String>) {
        let (mut clauses, mut values) = build_sql_conditions(
            &[
                (&self.community_name, "c.name::text"),
                (&self.policy_name, "np.name::text"),
                (&self.host, "h.name::text"),
            ],
            &None,
            &[],
        );

        let mut idx = values.len() + 1;
        for cond in &self.address {
            let (sql, val, consumed) = op_to_sql(&cond.op, "host(ip.address)", &cond.value, idx);
            clauses.push(sql);
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
                "community_name" => {
                    validate_op("community_name", &op, FieldType::String)?;
                    filter.community_name.push(FilterCondition { op, value });
                }
                "policy_name" => {
                    validate_op("policy_name", &op, FieldType::String)?;
                    filter.policy_name.push(FilterCondition { op, value });
                }
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
