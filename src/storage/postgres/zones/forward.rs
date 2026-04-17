use std::collections::HashMap;

use chrono::Utc;
use diesel::{
    Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl, SelectableHelper, delete, insert_into, update,
};
use uuid::Uuid;

use crate::{
    db::{
        models::{ForwardZoneRow, NameServerRow},
        schema::{forward_zone_nameservers, forward_zones, nameservers},
    },
    domain::{
        pagination::{Page, PageRequest},
        types::{DnsName, SerialNumber},
        zone::{CreateForwardZone, ForwardZone, UpdateForwardZone},
    },
    errors::AppError,
};

use super::super::PostgresStorage;
use super::super::helpers::{map_unique, vec_to_page};

impl PostgresStorage {
    pub(in crate::storage::postgres) fn load_forward_zone_nameservers(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<Vec<DnsName>, AppError> {
        let rows = nameservers::table
            .inner_join(
                forward_zone_nameservers::table
                    .on(forward_zone_nameservers::nameserver_id.eq(nameservers::id)),
            )
            .filter(forward_zone_nameservers::zone_id.eq(zone_id))
            .select(NameServerRow::as_select())
            .order(nameservers::name.asc())
            .load::<NameServerRow>(connection)?;

        rows.into_iter()
            .map(|row| DnsName::new(row.into_domain()?.name().as_str()))
            .collect()
    }

    pub(in crate::storage::postgres) fn query_forward_zones(
        connection: &mut PgConnection,
    ) -> Result<Vec<ForwardZone>, AppError> {
        let rows = forward_zones::table
            .select(ForwardZoneRow::as_select())
            .order(forward_zones::name.asc())
            .load::<ForwardZoneRow>(connection)?;

        // Bulk load all zone-nameserver pairs in one query (instead of N+1)
        let ns_pairs = forward_zone_nameservers::table
            .inner_join(nameservers::table)
            .select((forward_zone_nameservers::zone_id, nameservers::name))
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

    pub(super) fn list_forward_zones_impl(
        connection: &mut PgConnection,
        page: &PageRequest,
    ) -> Result<Page<ForwardZone>, AppError> {
        let items = Self::query_forward_zones(connection)?;
        Ok(vec_to_page(items, page))
    }

    pub(super) fn create_forward_zone_impl(
        connection: &mut PgConnection,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        let name = command.name().as_str().to_string();
        let primary_ns = command.primary_ns().as_str().to_string();
        let email = command.email().as_str().to_string();
        let serial_no = command.serial_no().as_i64();
        let refresh = command.refresh().as_i32();
        let retry = command.retry().as_i32();
        let expire = command.expire().as_i32();
        let soa_ttl = command.soa_ttl().as_i32();
        let default_ttl = command.default_ttl().as_i32();
        let nameservers = command.nameservers().to_vec();

        connection.transaction::<ForwardZone, AppError, _>(|connection| {
            let nameserver_ids = Self::lookup_nameserver_ids(connection, &nameservers)?;
            let row = insert_into(forward_zones::table)
                .values((
                    forward_zones::name.eq(&name),
                    forward_zones::primary_ns.eq(&primary_ns),
                    forward_zones::email.eq(&email),
                    forward_zones::serial_no.eq(serial_no),
                    forward_zones::refresh.eq(refresh),
                    forward_zones::retry.eq(retry),
                    forward_zones::expire.eq(expire),
                    forward_zones::soa_ttl.eq(soa_ttl),
                    forward_zones::default_ttl.eq(default_ttl),
                ))
                .returning(ForwardZoneRow::as_returning())
                .get_result(connection)
                .map_err(map_unique("forward zone already exists"))?;
            for nameserver_id in nameserver_ids {
                insert_into(forward_zone_nameservers::table)
                    .values((
                        forward_zone_nameservers::zone_id.eq(row.id()),
                        forward_zone_nameservers::nameserver_id.eq(nameserver_id),
                    ))
                    .execute(connection)?;
            }

            // Auto-create NS records for each nameserver
            for ns in &nameservers {
                use crate::domain::resource_records::{CreateRecordInstance, RecordOwnerKind};
                use crate::domain::types::RecordTypeName;

                let ns_data = serde_json::json!({"nsdname": ns.as_str()});
                Self::auto_create_record(connection, "NS", &name, ns_data, |tn, d| {
                    CreateRecordInstance::new(
                        RecordTypeName::new(tn)?,
                        RecordOwnerKind::ForwardZone,
                        &name,
                        None,
                        d,
                    )
                })?;
            }

            row.into_domain(nameservers)
        })
    }

    pub(in crate::storage::postgres) fn get_forward_zone_by_name_impl(
        connection: &mut PgConnection,
        name: &str,
    ) -> Result<ForwardZone, AppError> {
        let row = forward_zones::table
            .filter(forward_zones::name.eq(name))
            .select(ForwardZoneRow::as_select())
            .first::<ForwardZoneRow>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found(format!("forward zone '{}' was not found", name)))?;
        let nameservers = Self::load_forward_zone_nameservers(connection, row.id())?;
        row.into_domain(nameservers)
    }

    pub(super) fn update_forward_zone_impl(
        connection: &mut PgConnection,
        name: &str,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        connection.transaction::<ForwardZone, AppError, _>(|connection| {
            // Fetch the existing zone
            let row = forward_zones::table
                .filter(forward_zones::name.eq(name))
                .select(ForwardZoneRow::as_select())
                .first::<ForwardZoneRow>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("forward zone '{}' was not found", name))
                })?;

            let zone_id = row.id();
            let old_serial = row.serial_no();
            let old_nameservers = Self::load_forward_zone_nameservers(connection, zone_id)?;
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
            update(forward_zones::table.filter(forward_zones::id.eq(zone_id)))
                .set((
                    forward_zones::primary_ns.eq(&new_primary_ns),
                    forward_zones::email.eq(&new_email),
                    forward_zones::refresh.eq(new_refresh),
                    forward_zones::retry.eq(new_retry),
                    forward_zones::expire.eq(new_expire),
                    forward_zones::soa_ttl.eq(new_soa_ttl),
                    forward_zones::default_ttl.eq(new_default_ttl),
                    forward_zones::serial_no.eq(next_serial.as_i64()),
                    forward_zones::serial_no_updated_at.eq(diesel::dsl::now),
                    forward_zones::updated.eq(true),
                    forward_zones::updated_at.eq(diesel::dsl::now),
                ))
                .execute(connection)?;

            // Update nameservers if provided or if primary_ns changed
            if command.nameservers.is_some() || command.primary_ns.is_some() {
                delete(
                    forward_zone_nameservers::table
                        .filter(forward_zone_nameservers::zone_id.eq(zone_id)),
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
                    insert_into(forward_zone_nameservers::table)
                        .values((
                            forward_zone_nameservers::zone_id.eq(zone_id),
                            forward_zone_nameservers::nameserver_id.eq(nameserver_id),
                        ))
                        .execute(connection)?;
                }
            }

            // Re-fetch the updated zone
            let updated_row = forward_zones::table
                .filter(forward_zones::id.eq(zone_id))
                .select(ForwardZoneRow::as_select())
                .first::<ForwardZoneRow>(connection)?;

            let nameservers = Self::load_forward_zone_nameservers(connection, updated_row.id())?;
            updated_row.into_domain(nameservers)
        })
    }

    pub(super) fn delete_forward_zone_impl(
        connection: &mut PgConnection,
        name: &str,
    ) -> Result<(), AppError> {
        let deleted = delete(forward_zones::table.filter(forward_zones::name.eq(name)))
            .execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found(format!(
                "forward zone '{}' was not found",
                name
            )));
        }
        Ok(())
    }

    pub(super) fn bump_forward_zone_serial_impl(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<ForwardZone, AppError> {
        connection.transaction::<ForwardZone, AppError, _>(|connection| {
            let row = forward_zones::table
                .filter(forward_zones::id.eq(zone_id))
                .select(ForwardZoneRow::as_select())
                .first::<ForwardZoneRow>(connection)
                .optional()?
                .ok_or_else(|| AppError::not_found("forward zone not found"))?;

            let current_serial = SerialNumber::new(
                u64::try_from(row.serial_no())
                    .map_err(|_| AppError::internal("invalid serial number in database"))?,
            )?;
            let next_serial = current_serial.next_rfc1912(Utc::now().date_naive())?;

            update(forward_zones::table.filter(forward_zones::id.eq(zone_id)))
                .set((
                    forward_zones::serial_no.eq(next_serial.as_i64()),
                    forward_zones::serial_no_updated_at.eq(diesel::dsl::now),
                    forward_zones::updated.eq(true),
                    forward_zones::updated_at.eq(diesel::dsl::now),
                ))
                .execute(connection)?;

            let updated_row = forward_zones::table
                .filter(forward_zones::id.eq(zone_id))
                .select(ForwardZoneRow::as_select())
                .first::<ForwardZoneRow>(connection)?;

            let nameservers = Self::load_forward_zone_nameservers(connection, updated_row.id())?;
            updated_row.into_domain(nameservers)
        })
    }
}
