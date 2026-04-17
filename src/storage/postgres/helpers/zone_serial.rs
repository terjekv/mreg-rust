use diesel::{ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl};
use uuid::Uuid;

use crate::{domain::types::DnsName, errors::AppError};

use super::super::PostgresStorage;

impl PostgresStorage {
    /// Bump the zone serial within an existing transaction.
    /// Tries forward_zones first, then reverse_zones.
    pub(in crate::storage::postgres) fn bump_zone_serial_tx(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<(), AppError> {
        use crate::db::schema::{forward_zones, reverse_zones};

        let updated = diesel::update(forward_zones::table.filter(forward_zones::id.eq(zone_id)))
            .set((
                forward_zones::serial_no.eq(forward_zones::serial_no + 1i64),
                forward_zones::serial_no_updated_at.eq(diesel::dsl::now),
                forward_zones::updated.eq(true),
                forward_zones::updated_at.eq(diesel::dsl::now),
            ))
            .execute(connection)?;

        if updated == 0 {
            diesel::update(reverse_zones::table.filter(reverse_zones::id.eq(zone_id)))
                .set((
                    reverse_zones::serial_no.eq(reverse_zones::serial_no + 1i64),
                    reverse_zones::serial_no_updated_at.eq(diesel::dsl::now),
                    reverse_zones::updated.eq(true),
                    reverse_zones::updated_at.eq(diesel::dsl::now),
                ))
                .execute(connection)?;
        }

        Ok(())
    }

    /// Look up nameserver IDs for a slice of DNS names in a single query.
    /// Returns IDs in the same order as the input names.
    /// Returns NotFound if any name is missing from the nameservers table.
    pub(in crate::storage::postgres) fn lookup_nameserver_ids(
        connection: &mut PgConnection,
        names: &[DnsName],
    ) -> Result<Vec<Uuid>, AppError> {
        use crate::db::schema::nameservers;

        if names.is_empty() {
            return Ok(Vec::new());
        }

        let name_strs: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
        let rows: Vec<(String, Uuid)> = nameservers::table
            .filter(nameservers::name.eq_any(&name_strs))
            .select((nameservers::name, nameservers::id))
            .load(connection)?;

        names
            .iter()
            .map(|name| {
                rows.iter()
                    .find(|(n, _)| n == name.as_str())
                    .map(|(_, id)| *id)
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "nameserver '{}' does not exist",
                            name.as_str()
                        ))
                    })
            })
            .collect()
    }
}
