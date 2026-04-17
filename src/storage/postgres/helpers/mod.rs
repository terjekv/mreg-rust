//! Shared helpers for the PostgreSQL storage backend.
//!
//! This module is split into focused sub-modules:
//!
//! - `dynamic_query` -- binding a variable number of text parameters to raw SQL
//! - `pagination` -- cursor-based pagination over result sets
//! - `record_owner` -- resolving record owners (hosts, zones, delegations)
//! - `record_types` -- builtin record type seeding, schema helpers, row types
//! - `zone_serial` -- zone serial bumps and nameserver lookups

mod dynamic_query;
mod pagination;
mod record_owner;
mod record_types;
mod zone_serial;

use diesel::{
    QueryableByName,
    sql_types::{Text, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::errors::AppError;

// ---------------------------------------------------------------------------
// Re-exports -- preserve the existing public surface so that sibling modules
// (hosts.rs, records.rs, etc.) continue to compile with the same imports.
// ---------------------------------------------------------------------------

pub(super) use dynamic_query::{run_count_query, run_dynamic_query};
pub(super) use pagination::{paginate_simple, rows_to_page, vec_to_page};
pub(super) use record_types::{IntSentinelRow, record_owner_kind_value, record_type_storage_parts};

// ---------------------------------------------------------------------------
// Small shared utilities that don't warrant their own file
// ---------------------------------------------------------------------------

pub(super) fn map_unique(message: &'static str) -> impl FnOnce(diesel::result::Error) -> AppError {
    move |error| match error {
        diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        ) => AppError::conflict(message),
        other => AppError::internal(other),
    }
}

// ---------------------------------------------------------------------------
// Shared row types used by multiple sub-modules
// ---------------------------------------------------------------------------

#[derive(QueryableByName)]
pub(super) struct TextValueRow {
    #[diesel(sql_type = Text)]
    pub value: String,
}

#[derive(QueryableByName)]
pub(super) struct NameAndIdRow {
    #[diesel(sql_type = SqlUuid)]
    pub id: Uuid,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    pub name: String,
}
