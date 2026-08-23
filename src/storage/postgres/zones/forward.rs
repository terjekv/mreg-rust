use std::collections::HashMap;

use chrono::Utc;
use diesel::{
    Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl, SelectableHelper, delete, insert_into, sql_query, sql_types::Uuid as SqlUuid,
    update,
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
use super::super::helpers::{map_unique, sort_and_vec_to_page_by};

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

    pub(in crate::storage::postgres) fn list_forward_zones_impl(
        connection: &mut PgConnection,
        page: &PageRequest,
    ) -> Result<Page<ForwardZone>, AppError> {
        let items = Self::query_forward_zones(connection)?;
        sort_and_vec_to_page_by(
            items,
            page,
            &["name", "created_at", "updated_at"],
            |item, field| match field {
                "created_at" => item.created_at().to_rfc3339(),
                "updated_at" => item.updated_at().to_rfc3339(),
                _ => item.name().as_str().to_string(),
            },
        )
    }

    pub(in crate::storage::postgres) fn create_forward_zone_impl(
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
        let soa_record_ttl = command.soa_record_ttl().as_i32();
        let negative_ttl = command.negative_ttl().as_i32();
        let default_ttl = command.default_ttl().as_i32();
        let nameservers = command.nameservers().to_vec();

        connection.transaction::<ForwardZone, AppError, _>(|connection| {
            if sql_query("SELECT id FROM reverse_zones WHERE name = $1")
                .bind::<diesel::sql_types::Text, _>(&name)
                .get_result::<crate::db::models::UuidRow>(connection)
                .optional()?
                .is_some()
            {
                return Err(AppError::conflict(format!(
                    "zone '{}' already exists as a reverse zone",
                    name
                )));
            }
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
                    forward_zones::soa_record_ttl.eq(soa_record_ttl),
                    forward_zones::negative_ttl.eq(negative_ttl),
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

            Self::reconcile_managed_forward_records_for_zone(connection, row.id(), &name)?;

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

    pub(in crate::storage::postgres) fn update_forward_zone_impl(
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
            let new_soa_record_ttl = command
                .soa_record_ttl
                .unwrap_or(old_zone.soa_record_ttl())
                .as_i32();
            let new_negative_ttl = command
                .negative_ttl
                .unwrap_or(old_zone.negative_ttl())
                .as_i32();
            let new_default_ttl = command
                .default_ttl
                .unwrap_or(old_zone.default_ttl())
                .as_i32();

            // Bump serial
            let current_serial = SerialNumber::new(
                u32::try_from(old_serial)
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
                    forward_zones::soa_record_ttl.eq(new_soa_record_ttl),
                    forward_zones::negative_ttl.eq(new_negative_ttl),
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

                use crate::domain::{
                    resource_records::{CreateRecordInstance, RecordOwnerKind},
                    types::{RecordTypeName, record_type_names},
                };
                let owner = DnsName::new(name)?;
                Self::delete_records_by_owner_name_and_type_in_conn(
                    connection,
                    &owner,
                    &record_type_names::ns(),
                )?;
                for nameserver in &normalized {
                    Self::auto_create_record(
                        connection,
                        "NS",
                        name,
                        serde_json::json!({"nsdname": nameserver.as_str()}),
                        |type_name, data| {
                            CreateRecordInstance::new(
                                RecordTypeName::new(type_name)?,
                                RecordOwnerKind::ForwardZone,
                                name,
                                None,
                                data,
                            )
                        },
                    )?;
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

    pub(in crate::storage::postgres) fn delete_forward_zone_impl(
        connection: &mut PgConnection,
        name: &str,
    ) -> Result<(), AppError> {
        connection.transaction::<(), AppError, _>(|connection| {
            let zone_id = forward_zones::table
                .filter(forward_zones::name.eq(name))
                .select(forward_zones::id)
                .first::<Uuid>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("forward zone '{}' was not found", name))
                })?;
            sql_query("DELETE FROM records WHERE zone_id = $1")
                .bind::<SqlUuid, _>(zone_id)
                .execute(connection)?;
            sql_query("DELETE FROM rrsets WHERE zone_id = $1")
                .bind::<SqlUuid, _>(zone_id)
                .execute(connection)?;
            delete(forward_zones::table.filter(forward_zones::id.eq(zone_id)))
                .execute(connection)
                .map_err(|error| match error {
                    diesel::result::Error::DatabaseError(
                        diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                        _,
                    ) => AppError::conflict("forward zone still contains hosts"),
                    other => AppError::internal(other),
                })?;
            Ok(())
        })
    }

    pub(in crate::storage::postgres) fn bump_forward_zone_serial_impl(
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
                u32::try_from(row.serial_no())
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
