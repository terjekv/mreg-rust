use diesel::{OptionalExtension, PgConnection, QueryDsl, RunQueryDsl, sql_query, sql_types::Text};
use uuid::Uuid;

use crate::{
    domain::{resource_records::RecordOwnerKind, types::DnsName},
    errors::AppError,
};

use super::super::PostgresStorage;
use super::NameAndIdRow;

impl PostgresStorage {
    #[allow(clippy::type_complexity)]
    pub(in crate::storage::postgres) fn resolve_record_owner(
        connection: &mut PgConnection,
        owner_kind: Option<&RecordOwnerKind>,
        anchor_name: Option<&str>,
        owner_name: &DnsName,
    ) -> Result<(Option<Uuid>, Option<String>, Option<Uuid>), AppError> {
        use crate::db::schema::{
            forward_zone_delegations, forward_zones, hosts, nameservers, reverse_zone_delegations,
            reverse_zones,
        };
        use diesel::ExpressionMethods;

        let Some(owner_kind) = owner_kind else {
            return Ok((
                None,
                None,
                Self::best_matching_zone_for_owner_name(connection, owner_name)?,
            ));
        };

        let anchor_name = anchor_name.unwrap_or(owner_name.as_str());
        match owner_kind {
            RecordOwnerKind::Host => {
                let row = hosts::table
                    .filter(hosts::name.eq(anchor_name))
                    .select((hosts::id, hosts::name, hosts::zone_id))
                    .first::<(Uuid, String, Option<Uuid>)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("host '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), row.2))
            }
            RecordOwnerKind::ForwardZone => {
                let row = forward_zones::table
                    .filter(forward_zones::name.eq(anchor_name))
                    .select((forward_zones::id, forward_zones::name))
                    .first::<(Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("forward zone '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), Some(row.0)))
            }
            RecordOwnerKind::ReverseZone => {
                let row = reverse_zones::table
                    .filter(reverse_zones::name.eq(anchor_name))
                    .select((reverse_zones::id, reverse_zones::name))
                    .first::<(Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("reverse zone '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), Some(row.0)))
            }
            RecordOwnerKind::NameServer => {
                let row = nameservers::table
                    .filter(nameservers::name.eq(anchor_name))
                    .select((nameservers::id, nameservers::name))
                    .first::<(Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("nameserver '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), None))
            }
            RecordOwnerKind::ForwardZoneDelegation => {
                let row = forward_zone_delegations::table
                    .filter(forward_zone_delegations::name.eq(anchor_name))
                    .select((
                        forward_zone_delegations::id,
                        forward_zone_delegations::zone_id,
                        forward_zone_delegations::name,
                    ))
                    .first::<(Uuid, Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "forward zone delegation '{}' was not found",
                            anchor_name
                        ))
                    })?;
                let owner = owner_name.as_str();
                if owner != row.2 && !owner.ends_with(&format!(".{}", row.2)) {
                    return Err(AppError::validation(format!(
                        "owner name '{}' is not within delegation '{}'",
                        owner, row.2
                    )));
                }
                Ok((Some(row.0), Some(row.2), Some(row.1)))
            }
            RecordOwnerKind::ReverseZoneDelegation => {
                let row = reverse_zone_delegations::table
                    .filter(reverse_zone_delegations::name.eq(anchor_name))
                    .select((
                        reverse_zone_delegations::id,
                        reverse_zone_delegations::zone_id,
                        reverse_zone_delegations::name,
                    ))
                    .first::<(Uuid, Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "reverse zone delegation '{}' was not found",
                            anchor_name
                        ))
                    })?;
                let owner = owner_name.as_str();
                if owner != row.2 && !owner.ends_with(&format!(".{}", row.2)) {
                    return Err(AppError::validation(format!(
                        "owner name '{}' is not within delegation '{}'",
                        owner, row.2
                    )));
                }
                Ok((Some(row.0), Some(row.2), Some(row.1)))
            }
        }
    }

    pub(in crate::storage::postgres) fn best_matching_zone_for_owner_name(
        connection: &mut PgConnection,
        owner_name: &DnsName,
    ) -> Result<Option<Uuid>, AppError> {
        let candidate = owner_name.as_str();
        let rows = sql_query(
            "SELECT id, name::text AS name
             FROM (
                SELECT id, name FROM forward_zones
                UNION ALL
                SELECT id, name FROM reverse_zones
             ) zones
             WHERE lower($1::text) = lower(name::text)
                OR lower($1::text) LIKE '%.' || lower(name::text)
             ORDER BY length(name::text) DESC",
        )
        .bind::<Text, _>(candidate)
        .load::<NameAndIdRow>(connection)?;

        Ok(rows.into_iter().next().map(|row| row.id))
    }
}
