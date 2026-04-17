use diesel::{OptionalExtension, PgConnection, QueryDsl, RunQueryDsl};
use uuid::Uuid;

use crate::{domain::types::DnsName, errors::AppError};

use super::super::PostgresStorage;

impl PostgresStorage {
    /// Best-effort zone serial bump within an existing transaction.
    /// Tries forward_zones first, then reverse_zones. Silently ignores errors.
    pub(in crate::storage::postgres) fn bump_zone_serial_tx(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) {
        use crate::db::schema::{forward_zones, reverse_zones};
        use diesel::ExpressionMethods;

        // Try forward zone first
        let _ = diesel::update(forward_zones::table.filter(forward_zones::id.eq(zone_id)))
            .set((
                forward_zones::serial_no.eq(forward_zones::serial_no + 1i64),
                forward_zones::serial_no_updated_at.eq(diesel::dsl::now),
                forward_zones::updated.eq(true),
                forward_zones::updated_at.eq(diesel::dsl::now),
            ))
            .execute(connection)
            .ok()
            .and_then(|updated| {
                if updated == 0 {
                    // Try reverse zone
                    diesel::update(reverse_zones::table.filter(reverse_zones::id.eq(zone_id)))
                        .set((
                            reverse_zones::serial_no.eq(reverse_zones::serial_no + 1i64),
                            reverse_zones::serial_no_updated_at.eq(diesel::dsl::now),
                            reverse_zones::updated.eq(true),
                            reverse_zones::updated_at.eq(diesel::dsl::now),
                        ))
                        .execute(connection)
                        .ok()
                } else {
                    Some(updated)
                }
            });
    }

    pub(in crate::storage::postgres) fn lookup_nameserver_ids(
        connection: &mut PgConnection,
        names: &[DnsName],
    ) -> Result<Vec<Uuid>, AppError> {
        use crate::db::schema::nameservers;
        use diesel::ExpressionMethods;

        let mut ids = Vec::with_capacity(names.len());
        for name in names {
            let id = nameservers::table
                .filter(nameservers::name.eq(name.as_str()))
                .select(nameservers::id)
                .first::<Uuid>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("nameserver '{}' does not exist", name.as_str()))
                })?;
            ids.push(id);
        }
        Ok(ids)
    }
}
