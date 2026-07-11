use std::collections::HashMap;

use chrono::Utc;
use diesel::{
    Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl, SelectableHelper, delete, insert_into, sql_query,
    sql_types::{Integer, Nullable, Text, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    db::{
        models::{NameServerRow, ReverseZoneRow},
        schema::{nameservers, reverse_zone_nameservers},
    },
    domain::{
        pagination::{Page, PageRequest},
        types::{DnsName, SerialNumber},
        zone::{CreateReverseZone, ReverseZone, UpdateReverseZone},
    },
    errors::AppError,
};

use super::super::PostgresStorage;
use super::super::helpers::{map_unique, vec_to_page};

impl PostgresStorage {
    pub(in crate::storage::postgres) fn load_reverse_zone_nameservers(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<Vec<DnsName>, AppError> {
        let rows = nameservers::table
            .inner_join(
                reverse_zone_nameservers::table
                    .on(reverse_zone_nameservers::nameserver_id.eq(nameservers::id)),
            )
            .filter(reverse_zone_nameservers::zone_id.eq(zone_id))
            .select(NameServerRow::as_select())
            .order(nameservers::name.asc())
            .load::<NameServerRow>(connection)?;

        rows.into_iter()
            .map(|row| DnsName::new(row.into_domain()?.name().as_str()))
            .collect()
    }

    pub(in crate::storage::postgres) fn query_reverse_zones(
        connection: &mut PgConnection,
    ) -> Result<Vec<ReverseZone>, AppError> {
        let rows = sql_query(
            "SELECT id, name::text AS name, network::text AS network, updated,
                    primary_ns::text AS primary_ns, email::text AS email, serial_no,
                    serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                    created_at, updated_at
             FROM reverse_zones
             ORDER BY name",
        )
        .load::<ReverseZoneRow>(connection)?;

        // Bulk load all zone-nameserver pairs in one query (instead of N+1)
        let ns_pairs = reverse_zone_nameservers::table
            .inner_join(nameservers::table)
            .select((reverse_zone_nameservers::zone_id, nameservers::name))
            .order(nameservers::name.asc())
            .load::<(Uuid, String)>(connection)?;

        let mut ns_map: HashMap<Uuid, Vec<DnsName>> = HashMap::new();
        for (zone_id, name) in ns_pairs {
            ns_map.entry(zone_id).or_default().push(DnsName::new(name)?);
        }

        rows.into_iter()
            .map(|row| {
                let ns = ns_map.remove(&row.id()).unwrap_or_default();
                row.into_domain(ns)
            })
            .collect()
    }

    pub(in crate::storage::postgres) fn list_reverse_zones_impl(
        connection: &mut PgConnection,
        page: &PageRequest,
    ) -> Result<Page<ReverseZone>, AppError> {
        let items = Self::query_reverse_zones(connection)?;
        Ok(vec_to_page(items, page))
    }

    pub(in crate::storage::postgres) fn create_reverse_zone_impl(
        connection: &mut PgConnection,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        let name = command.name().as_str().to_string();
        let network = command.network().map(|value| value.as_str());
        let primary_ns = command.primary_ns().as_str().to_string();
        let email = command.email().as_str().to_string();
        let serial_no = command.serial_no().as_i64();
        let refresh = command.refresh().as_i32();
        let retry = command.retry().as_i32();
        let expire = command.expire().as_i32();
        let soa_ttl = command.soa_ttl().as_i32();
        let default_ttl = command.default_ttl().as_i32();
        let nameservers = command.nameservers().to_vec();

        connection.transaction::<ReverseZone, AppError, _>(|connection| {
            let nameserver_ids = Self::lookup_nameserver_ids(connection, &nameservers)?;
            let row = sql_query(
                "INSERT INTO reverse_zones
                    (name, network, primary_ns, email, serial_no, refresh, retry, expire, soa_ttl, default_ttl)
                 VALUES
                    ($1, $2::cidr, $3, $4, $5, $6, $7, $8, $9, $10)
                 RETURNING id, name::text AS name, network::text AS network, updated,
                           primary_ns::text AS primary_ns, email::text AS email, serial_no,
                           serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                           created_at, updated_at",
            )
            .bind::<Text, _>(&name)
            .bind::<Nullable<Text>, _>(network)
            .bind::<Text, _>(&primary_ns)
            .bind::<Text, _>(&email)
            .bind::<diesel::sql_types::BigInt, _>(serial_no)
            .bind::<Integer, _>(refresh)
            .bind::<Integer, _>(retry)
            .bind::<Integer, _>(expire)
            .bind::<Integer, _>(soa_ttl)
            .bind::<Integer, _>(default_ttl)
            .get_result::<ReverseZoneRow>(connection)
            .map_err(map_unique("reverse zone already exists"))?;
            for nameserver_id in nameserver_ids {
                insert_into(reverse_zone_nameservers::table)
                    .values((
                        reverse_zone_nameservers::zone_id.eq(row.id()),
                        reverse_zone_nameservers::nameserver_id.eq(nameserver_id),
                    ))
                    .execute(connection)?;
            }

            // Auto-create NS records for each nameserver
            for ns in &nameservers {
                use crate::domain::resource_records::{CreateRecordInstance, RecordOwnerKind};
                use crate::domain::types::RecordTypeName;

                let ns_data = serde_json::json!({"nsdname": ns.as_str()});
                Self::auto_create_record(
                    connection,
                    "NS",
                    &name,
                    ns_data,
                    |tn, d| {
                        CreateRecordInstance::new(
                            RecordTypeName::new(tn)?,
                            RecordOwnerKind::ReverseZone,
                            &name,
                            None,
                            d,
                        )
                    },
                )?;
            }

            row.into_domain(nameservers)
        })
    }

    pub(in crate::storage::postgres) fn get_reverse_zone_by_name_impl(
        connection: &mut PgConnection,
        name: &str,
    ) -> Result<ReverseZone, AppError> {
        let row = sql_query(
            "SELECT id, name::text AS name, network::text AS network, updated,
                    primary_ns::text AS primary_ns, email::text AS email, serial_no,
                    serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                    created_at, updated_at
             FROM reverse_zones
             WHERE name = $1",
        )
        .bind::<Text, _>(name)
        .get_result::<ReverseZoneRow>(connection)
        .map_err(|_| AppError::not_found(format!("reverse zone '{}' was not found", name)))?;
        let nameservers = Self::load_reverse_zone_nameservers(connection, row.id())?;
        row.into_domain(nameservers)
    }

    pub(in crate::storage::postgres) fn update_reverse_zone_impl(
        connection: &mut PgConnection,
        name: &str,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        connection.transaction::<ReverseZone, AppError, _>(|connection| {
            // Fetch the existing zone
            let row = sql_query(
                "SELECT id, name::text AS name, network::text AS network, updated,
                        primary_ns::text AS primary_ns, email::text AS email, serial_no,
                        serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                        created_at, updated_at
                 FROM reverse_zones
                 WHERE name = $1",
            )
            .bind::<Text, _>(name)
            .get_result::<ReverseZoneRow>(connection)
            .map_err(|_| AppError::not_found(format!("reverse zone '{}' was not found", name)))?;

            let zone_id = row.id();
            let old_serial = row.serial_no();
            let old_nameservers = Self::load_reverse_zone_nameservers(connection, zone_id)?;
            let old_zone = row.into_domain(old_nameservers.clone())?;

            // Compute new values, falling back to existing
            let new_primary_ns = command
                .primary_ns
                .as_ref()
                .map(|v| v.as_str().to_string())
                .unwrap_or_else(|| old_zone.primary_ns().as_str().to_string());
            let new_email = command
                .email
                .as_ref()
                .map(|v| v.as_str().to_string())
                .unwrap_or_else(|| old_zone.email().as_str().to_string());
            let new_refresh = command.refresh.unwrap_or(old_zone.refresh()).as_i32();
            let new_retry = command.retry.unwrap_or(old_zone.retry()).as_i32();
            let new_expire = command.expire.unwrap_or(old_zone.expire()).as_i32();
            let new_soa_ttl = command.soa_ttl.unwrap_or(old_zone.soa_ttl()).as_i32();
            let new_default_ttl = command
                .default_ttl
                .unwrap_or(old_zone.default_ttl())
                .as_i32();

            // Bump serial
            let current_serial = SerialNumber::new(
                u64::try_from(old_serial)
                    .map_err(|_| AppError::internal("invalid serial number in database"))?,
            )?;
            let next_serial = current_serial.next_rfc1912(Utc::now().date_naive())?;

            // Update the zone row
            sql_query(
                "UPDATE reverse_zones
                 SET primary_ns = $1, email = $2,
                     refresh = $3, retry = $4, expire = $5,
                     soa_ttl = $6, default_ttl = $7,
                     serial_no = $8, serial_no_updated_at = now(),
                     updated = true, updated_at = now()
                 WHERE id = $9",
            )
            .bind::<Text, _>(&new_primary_ns)
            .bind::<Text, _>(&new_email)
            .bind::<Integer, _>(new_refresh)
            .bind::<Integer, _>(new_retry)
            .bind::<Integer, _>(new_expire)
            .bind::<Integer, _>(new_soa_ttl)
            .bind::<Integer, _>(new_default_ttl)
            .bind::<diesel::sql_types::BigInt, _>(next_serial.as_i64())
            .bind::<SqlUuid, _>(zone_id)
            .execute(connection)?;

            // Update nameservers if provided or if primary_ns changed
            if command.nameservers.is_some() || command.primary_ns.is_some() {
                delete(
                    reverse_zone_nameservers::table
                        .filter(reverse_zone_nameservers::zone_id.eq(zone_id)),
                )
                .execute(connection)?;

                let base_nameservers = command.nameservers.as_ref().unwrap_or(&old_nameservers);

                // Normalize nameservers to include primary_ns
                let primary_ns_dns = DnsName::new(&new_primary_ns)?;
                let mut normalized = vec![primary_ns_dns.clone()];
                for ns in base_nameservers {
                    if !normalized.iter().any(|existing| existing == ns) {
                        normalized.push(ns.clone());
                    }
                }

                let nameserver_ids = Self::lookup_nameserver_ids(connection, &normalized)?;
                for nameserver_id in nameserver_ids {
                    insert_into(reverse_zone_nameservers::table)
                        .values((
                            reverse_zone_nameservers::zone_id.eq(zone_id),
                            reverse_zone_nameservers::nameserver_id.eq(nameserver_id),
                        ))
                        .execute(connection)?;
                }
            }

            // Re-fetch the updated zone
            let updated_row = sql_query(
                "SELECT id, name::text AS name, network::text AS network, updated,
                        primary_ns::text AS primary_ns, email::text AS email, serial_no,
                        serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                        created_at, updated_at
                 FROM reverse_zones WHERE id = $1",
            )
            .bind::<SqlUuid, _>(zone_id)
            .get_result::<ReverseZoneRow>(connection)?;

            let nameservers = Self::load_reverse_zone_nameservers(connection, updated_row.id())?;
            updated_row.into_domain(nameservers)
        })
    }

    pub(in crate::storage::postgres) fn delete_reverse_zone_impl(
        connection: &mut PgConnection,
        name: &str,
    ) -> Result<(), AppError> {
        let deleted = sql_query("DELETE FROM reverse_zones WHERE name = $1")
            .bind::<Text, _>(name)
            .execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found(format!(
                "reverse zone '{}' was not found",
                name
            )));
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn bump_reverse_zone_serial_impl(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<ReverseZone, AppError> {
        connection.transaction::<ReverseZone, AppError, _>(|connection| {
            let row = sql_query(
                "SELECT id, name::text AS name, network::text AS network, updated,
                        primary_ns::text AS primary_ns, email::text AS email, serial_no,
                        serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                        created_at, updated_at
                 FROM reverse_zones WHERE id = $1",
            )
            .bind::<SqlUuid, _>(zone_id)
            .get_result::<ReverseZoneRow>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found("reverse zone not found"))?;

            let current_serial = SerialNumber::new(
                u64::try_from(row.serial_no())
                    .map_err(|_| AppError::internal("invalid serial number in database"))?,
            )?;
            let next_serial = current_serial.next_rfc1912(Utc::now().date_naive())?;

            sql_query(
                "UPDATE reverse_zones
                 SET serial_no = $1, serial_no_updated_at = now(),
                     updated = true, updated_at = now()
                 WHERE id = $2",
            )
            .bind::<diesel::sql_types::BigInt, _>(next_serial.as_i64())
            .bind::<SqlUuid, _>(zone_id)
            .execute(connection)?;

            let updated_row = sql_query(
                "SELECT id, name::text AS name, network::text AS network, updated,
                        primary_ns::text AS primary_ns, email::text AS email, serial_no,
                        serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                        created_at, updated_at
                 FROM reverse_zones WHERE id = $1",
            )
            .bind::<SqlUuid, _>(zone_id)
            .get_result::<ReverseZoneRow>(connection)?;

            let nameservers = Self::load_reverse_zone_nameservers(connection, updated_row.id())?;
            updated_row.into_domain(nameservers)
        })
    }
}
