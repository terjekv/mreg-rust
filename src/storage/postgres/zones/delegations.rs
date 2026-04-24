use std::collections::HashMap;

use diesel::{
    Connection, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl,
    SelectableHelper, delete, insert_into, sql_query,
    sql_types::{Text, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    db::{
        models::{ForwardDelegationRow, ReverseDelegationRow, UuidRow},
        schema::{
            forward_zone_delegation_nameservers, forward_zone_delegations, forward_zones,
            nameservers, reverse_zone_delegation_nameservers, reverse_zone_delegations,
        },
    },
    domain::{
        pagination::{Page, PageRequest},
        types::DnsName,
        zone::{
            CreateForwardZoneDelegation, CreateReverseZoneDelegation, ForwardZoneDelegation,
            ReverseZoneDelegation,
        },
    },
    errors::AppError,
};

use super::super::PostgresStorage;
use super::super::helpers::{map_unique, vec_to_page};

impl PostgresStorage {
    pub(in crate::storage::postgres) fn list_forward_zone_delegations_impl(
        connection: &mut PgConnection,
        zone_name: &str,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        let zone_id = forward_zones::table
            .filter(forward_zones::name.eq(zone_name))
            .select(forward_zones::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!("forward zone '{zone_name}' was not found"))
            })?;

        let rows = forward_zone_delegations::table
            .filter(forward_zone_delegations::zone_id.eq(zone_id))
            .select(ForwardDelegationRow::as_select())
            .order(forward_zone_delegations::name.asc())
            .load::<ForwardDelegationRow>(connection)?;

        let del_ids: Vec<Uuid> = rows.iter().map(|r| r.id()).collect();
        let ns_pairs = forward_zone_delegation_nameservers::table
            .inner_join(nameservers::table)
            .filter(forward_zone_delegation_nameservers::delegation_id.eq_any(&del_ids))
            .select((
                forward_zone_delegation_nameservers::delegation_id,
                nameservers::name,
            ))
            .order(nameservers::name.asc())
            .load::<(Uuid, String)>(connection)?;

        let mut ns_map: HashMap<Uuid, Vec<DnsName>> = HashMap::new();
        for (delegation_id, name) in ns_pairs {
            ns_map
                .entry(delegation_id)
                .or_default()
                .push(DnsName::new(name)?);
        }

        let items = rows
            .into_iter()
            .map(|row| {
                let ns = ns_map.remove(&row.id()).unwrap_or_default();
                row.into_forward_delegation(ns)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vec_to_page(items, page))
    }

    pub(in crate::storage::postgres) fn create_forward_zone_delegation_impl(
        connection: &mut PgConnection,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        let zone_name = command.zone_name().as_str().to_string();
        let name = command.name().as_str().to_string();
        let comment = command.comment().to_string();
        let nameservers = command.nameservers().to_vec();

        connection.transaction::<ForwardZoneDelegation, AppError, _>(|connection| {
            let zone_id = forward_zones::table
                .filter(forward_zones::name.eq(&zone_name))
                .select(forward_zones::id)
                .first::<Uuid>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("forward zone '{zone_name}' was not found"))
                })?;

            let row = insert_into(forward_zone_delegations::table)
                .values((
                    forward_zone_delegations::zone_id.eq(zone_id),
                    forward_zone_delegations::name.eq(&name),
                    forward_zone_delegations::comment.eq(&comment),
                ))
                .returning(ForwardDelegationRow::as_returning())
                .get_result(connection)
                .map_err(map_unique("forward zone delegation already exists"))?;

            let ns_ids = Self::lookup_nameserver_ids(connection, &nameservers)?;
            for ns_id in ns_ids {
                insert_into(forward_zone_delegation_nameservers::table)
                    .values((
                        forward_zone_delegation_nameservers::delegation_id.eq(row.id()),
                        forward_zone_delegation_nameservers::nameserver_id.eq(ns_id),
                    ))
                    .execute(connection)?;
            }

            // Auto-create NS records for the delegation
            for ns in &nameservers {
                use crate::domain::resource_records::{CreateRecordInstance, RecordOwnerKind};
                use crate::domain::types::RecordTypeName;

                let ns_data = serde_json::json!({"nsdname": ns.as_str()});
                Self::auto_create_record(connection, "NS", &name, ns_data, |tn, d| {
                    CreateRecordInstance::new(
                        RecordTypeName::new(tn)?,
                        RecordOwnerKind::ForwardZoneDelegation,
                        &name,
                        None,
                        d,
                    )
                })?;
            }

            // Bump parent zone serial
            Self::bump_zone_serial_tx(connection, zone_id)?;

            row.into_forward_delegation(nameservers)
        })
    }

    pub(in crate::storage::postgres) fn delete_forward_zone_delegation_impl(
        connection: &mut PgConnection,
        delegation_id: Uuid,
    ) -> Result<(), AppError> {
        connection.transaction::<(), AppError, _>(|connection| {
            // Look up the delegation's zone_id before deleting
            let zone_id = forward_zone_delegations::table
                .filter(forward_zone_delegations::id.eq(delegation_id))
                .select(forward_zone_delegations::zone_id)
                .first::<Uuid>(connection)
                .optional()?;

            // Delete associated records
            sql_query(
                "DELETE FROM records WHERE owner_id = $1",
            )
            .bind::<SqlUuid, _>(delegation_id)
            .execute(connection)?;
            sql_query(
                "DELETE FROM rrsets WHERE NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = rrsets.id)",
            )
            .execute(connection)?;

            let deleted = delete(forward_zone_delegations::table
                .filter(forward_zone_delegations::id.eq(delegation_id)))
                .execute(connection)?;
            if deleted == 0 {
                return Err(AppError::not_found("forward zone delegation not found"));
            }

            // Bump parent zone serial
            if let Some(zone_id) = zone_id {
                Self::bump_zone_serial_tx(connection, zone_id)?;
            }

            Ok(())
        })
    }

    pub(in crate::storage::postgres) fn list_reverse_zone_delegations_impl(
        connection: &mut PgConnection,
        zone_name: &str,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        let zone_id = sql_query("SELECT id FROM reverse_zones WHERE name = $1")
            .bind::<Text, _>(zone_name)
            .get_result::<UuidRow>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!("reverse zone '{zone_name}' was not found"))
            })?
            .id();

        let rows = reverse_zone_delegations::table
            .filter(reverse_zone_delegations::zone_id.eq(zone_id))
            .select(ReverseDelegationRow::as_select())
            .order(reverse_zone_delegations::name.asc())
            .load::<ReverseDelegationRow>(connection)?;

        let del_ids: Vec<Uuid> = rows.iter().map(|r| r.id()).collect();
        let ns_pairs = reverse_zone_delegation_nameservers::table
            .inner_join(nameservers::table)
            .filter(reverse_zone_delegation_nameservers::delegation_id.eq_any(&del_ids))
            .select((
                reverse_zone_delegation_nameservers::delegation_id,
                nameservers::name,
            ))
            .order(nameservers::name.asc())
            .load::<(Uuid, String)>(connection)?;

        let mut ns_map: HashMap<Uuid, Vec<DnsName>> = HashMap::new();
        for (delegation_id, name) in ns_pairs {
            ns_map
                .entry(delegation_id)
                .or_default()
                .push(DnsName::new(name)?);
        }

        let items = rows
            .into_iter()
            .map(|row| {
                let ns = ns_map.remove(&row.id()).unwrap_or_default();
                row.into_reverse_delegation(ns)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vec_to_page(items, page))
    }

    pub(in crate::storage::postgres) fn create_reverse_zone_delegation_impl(
        connection: &mut PgConnection,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        let zone_name = command.zone_name().as_str().to_string();
        let name = command.name().as_str().to_string();
        let comment = command.comment().to_string();
        let nameservers = command.nameservers().to_vec();

        connection.transaction::<ReverseZoneDelegation, AppError, _>(|connection| {
            let zone_id = sql_query("SELECT id FROM reverse_zones WHERE name = $1")
                .bind::<Text, _>(&zone_name)
                .get_result::<UuidRow>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("reverse zone '{zone_name}' was not found"))
                })?
                .id();

            let row = insert_into(reverse_zone_delegations::table)
                .values((
                    reverse_zone_delegations::zone_id.eq(zone_id),
                    reverse_zone_delegations::name.eq(&name),
                    reverse_zone_delegations::comment.eq(&comment),
                ))
                .returning(ReverseDelegationRow::as_returning())
                .get_result(connection)
                .map_err(map_unique("reverse zone delegation already exists"))?;

            let ns_ids = Self::lookup_nameserver_ids(connection, &nameservers)?;
            for ns_id in ns_ids {
                insert_into(reverse_zone_delegation_nameservers::table)
                    .values((
                        reverse_zone_delegation_nameservers::delegation_id.eq(row.id()),
                        reverse_zone_delegation_nameservers::nameserver_id.eq(ns_id),
                    ))
                    .execute(connection)?;
            }

            // Auto-create NS records for the delegation
            for ns in &nameservers {
                use crate::domain::resource_records::{CreateRecordInstance, RecordOwnerKind};
                use crate::domain::types::RecordTypeName;

                let ns_data = serde_json::json!({"nsdname": ns.as_str()});
                Self::auto_create_record(connection, "NS", &name, ns_data, |tn, d| {
                    CreateRecordInstance::new(
                        RecordTypeName::new(tn)?,
                        RecordOwnerKind::ReverseZoneDelegation,
                        &name,
                        None,
                        d,
                    )
                })?;
            }

            // Bump parent zone serial
            Self::bump_zone_serial_tx(connection, zone_id)?;

            row.into_reverse_delegation(nameservers)
        })
    }

    pub(in crate::storage::postgres) fn delete_reverse_zone_delegation_impl(
        connection: &mut PgConnection,
        delegation_id: Uuid,
    ) -> Result<(), AppError> {
        connection.transaction::<(), AppError, _>(|connection| {
            // Look up the delegation's zone_id before deleting
            let zone_id = reverse_zone_delegations::table
                .filter(reverse_zone_delegations::id.eq(delegation_id))
                .select(reverse_zone_delegations::zone_id)
                .first::<Uuid>(connection)
                .optional()?;

            // Delete associated records
            sql_query(
                "DELETE FROM records WHERE owner_id = $1",
            )
            .bind::<SqlUuid, _>(delegation_id)
            .execute(connection)?;
            sql_query(
                "DELETE FROM rrsets WHERE NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = rrsets.id)",
            )
            .execute(connection)?;

            let deleted = delete(reverse_zone_delegations::table
                .filter(reverse_zone_delegations::id.eq(delegation_id)))
                .execute(connection)?;
            if deleted == 0 {
                return Err(AppError::not_found("reverse zone delegation not found"));
            }

            // Bump parent zone serial
            if let Some(zone_id) = zone_id {
                Self::bump_zone_serial_tx(connection, zone_id)?;
            }

            Ok(())
        })
    }
}
