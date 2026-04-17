use std::collections::BTreeMap;

use async_trait::async_trait;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl,
    sql_query,
    sql_types::{Array, Integer, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    db::{
        models::{HostRow, IpAddressAssignmentRow},
        schema::{forward_zones, hosts},
    },
    domain::{
        attachment::{CreateAttachmentDhcpIdentifier, DhcpIdentifierFamily, DhcpIdentifierKind},
        filters::HostFilter,
        host::{
            AllocationPolicy, AssignIpAddress, CreateHost, Host, HostAuthContext,
            IpAddressAssignment, UpdateHost, UpdateIpAddress,
        },
        pagination::{Page, PageRequest},
        types::{
            CidrValue, DhcpPriority, Hostname, IpAddressValue, MacAddressValue, Ttl, ZoneName,
        },
    },
    errors::AppError,
    storage::HostStore,
};

use super::PostgresStorage;
use super::helpers::{map_unique, run_dynamic_query, vec_to_page};

/// Resolved values for a host update, computed from the command and the existing host.
struct ResolvedHostUpdate {
    name: String,
    ttl: Option<i32>,
    comment: String,
    zone_name: Option<String>,
    zone_id: Option<uuid::Uuid>,
}

#[derive(diesel::QueryableByName)]
struct HostAuthContextRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Nullable<Text>)]
    zone_name: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    ttl: Option<i32>,
    #[diesel(sql_type = Text)]
    comment: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: chrono::DateTime<chrono::Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: chrono::DateTime<chrono::Utc>,
    #[diesel(sql_type = Array<Text>)]
    addresses: Vec<String>,
    #[diesel(sql_type = Array<Text>)]
    networks: Vec<String>,
}

impl HostAuthContextRow {
    fn into_domain(self) -> Result<HostAuthContext, AppError> {
        let host = Host::restore(
            self.id,
            Hostname::new(self.name)?,
            self.zone_name.map(ZoneName::new).transpose()?,
            self.ttl.map(|value| Ttl::new(value as u32)).transpose()?,
            self.comment,
            self.created_at,
            self.updated_at,
        )?;
        let addresses = self
            .addresses
            .into_iter()
            .map(IpAddressValue::new)
            .collect::<Result<Vec<_>, _>>()?;
        let networks = self
            .networks
            .into_iter()
            .map(CidrValue::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HostAuthContext::new(host, addresses, networks))
    }
}

impl PostgresStorage {
    pub(super) fn query_hosts(connection: &mut PgConnection) -> Result<Vec<Host>, AppError> {
        let rows = sql_query(
            "SELECT h.id,
                    h.name::text AS name,
                    fz.name::text AS zone_name,
                    h.ttl,
                    h.comment,
                    h.created_at,
                    h.updated_at
             FROM hosts h
             LEFT JOIN forward_zones fz ON fz.id = h.zone_id
             ORDER BY h.name",
        )
        .load::<HostRow>(connection)?;
        rows.into_iter().map(HostRow::into_domain).collect()
    }

    pub(super) fn query_hosts_for_zone(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<Vec<Host>, AppError> {
        let rows = sql_query(
            "SELECT h.id,
                    h.name::text AS name,
                    fz.name::text AS zone_name,
                    h.ttl,
                    h.comment,
                    h.created_at,
                    h.updated_at
             FROM hosts h
             LEFT JOIN forward_zones fz ON fz.id = h.zone_id
             WHERE h.zone_id = $1
             ORDER BY h.name",
        )
        .bind::<diesel::sql_types::Uuid, _>(zone_id)
        .load::<HostRow>(connection)?;
        rows.into_iter().map(HostRow::into_domain).collect()
    }

    pub(super) fn query_ip_addresses_for_hosts(
        connection: &mut PgConnection,
        host_ids: &[Uuid],
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        if host_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows = sql_query(
            "SELECT ip.id,
                    ip.host_id,
                    ip.attachment_id,
                    host(ip.address) AS address,
                    ip.family::int AS family,
                    nw.id AS network_id,
                    ip.mac_address,
                    ip.created_at,
                    ip.updated_at
             FROM ip_addresses ip
             JOIN LATERAL (
               SELECT id
               FROM networks
               WHERE ip.address <<= network
               ORDER BY masklen(network) DESC
               LIMIT 1
             ) nw ON true
             WHERE ip.host_id = ANY($1)
             ORDER BY ip.address",
        )
        .bind::<diesel::sql_types::Array<diesel::sql_types::Uuid>, _>(host_ids)
        .load::<IpAddressAssignmentRow>(connection)?;
        rows.into_iter()
            .map(IpAddressAssignmentRow::into_domain)
            .collect()
    }

    pub(super) fn query_host_by_name(
        connection: &mut PgConnection,
        name: &Hostname,
    ) -> Result<Host, AppError> {
        sql_query(
            "SELECT h.id,
                    h.name::text AS name,
                    fz.name::text AS zone_name,
                    h.ttl,
                    h.comment,
                    h.created_at,
                    h.updated_at
             FROM hosts h
             LEFT JOIN forward_zones fz ON fz.id = h.zone_id
             WHERE h.name = $1",
        )
        .bind::<Text, _>(name.as_str())
        .get_result::<HostRow>(connection)
        .map_err(|_| AppError::not_found(format!("host '{}' was not found", name.as_str())))?
        .into_domain()
    }

    pub(super) fn query_host_auth_context(
        connection: &mut PgConnection,
        name: &Hostname,
    ) -> Result<HostAuthContext, AppError> {
        sql_query(
            "SELECT h.id,
                    h.name::text AS name,
                    fz.name::text AS zone_name,
                    h.ttl,
                    h.comment,
                    h.created_at,
                    h.updated_at,
                    COALESCE(
                        array_agg(DISTINCT host(ip.address))
                            FILTER (WHERE ip.address IS NOT NULL),
                        ARRAY[]::text[]
                    ) AS addresses,
                    COALESCE(
                        array_agg(DISTINCT nw.network::text)
                            FILTER (WHERE nw.network IS NOT NULL),
                        ARRAY[]::text[]
                    ) AS networks
             FROM hosts h
             LEFT JOIN forward_zones fz ON fz.id = h.zone_id
             LEFT JOIN ip_addresses ip ON ip.host_id = h.id
             LEFT JOIN LATERAL (
               SELECT network
               FROM networks
               WHERE ip.address <<= network
               ORDER BY masklen(network) DESC
               LIMIT 1
             ) nw ON true
             WHERE h.name = $1
             GROUP BY h.id, h.name, fz.name, h.ttl, h.comment, h.created_at, h.updated_at",
        )
        .bind::<Text, _>(name.as_str())
        .get_result::<HostAuthContextRow>(connection)
        .map_err(|_| AppError::not_found(format!("host '{}' was not found", name.as_str())))?
        .into_domain()
    }

    pub(super) fn query_ip_addresses(
        connection: &mut PgConnection,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        let rows = sql_query(
            "SELECT ip.id,
                    ip.host_id,
                    ip.attachment_id,
                    host(ip.address) AS address,
                    ip.family::int AS family,
                    nw.id AS network_id,
                    ip.mac_address,
                    ip.created_at,
                    ip.updated_at
             FROM ip_addresses ip
             JOIN LATERAL (
               SELECT id
               FROM networks
               WHERE ip.address <<= network
               ORDER BY masklen(network) DESC
               LIMIT 1
             ) nw ON true
             ORDER BY ip.address",
        )
        .load::<IpAddressAssignmentRow>(connection)?;
        rows.into_iter()
            .map(IpAddressAssignmentRow::into_domain)
            .collect()
    }

    pub(super) fn query_ip_addresses_for_host(
        connection: &mut PgConnection,
        name: &Hostname,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        let rows = sql_query(
            "SELECT ip.id,
                    ip.host_id,
                    ip.attachment_id,
                    host(ip.address) AS address,
                    ip.family::int AS family,
                    nw.id AS network_id,
                    ip.mac_address,
                    ip.created_at,
                    ip.updated_at
             FROM ip_addresses ip
             JOIN hosts h ON h.id = ip.host_id
             JOIN LATERAL (
               SELECT id
               FROM networks
               WHERE ip.address <<= network
               ORDER BY masklen(network) DESC
               LIMIT 1
             ) nw ON true
             WHERE h.name = $1
             ORDER BY ip.address",
        )
        .bind::<Text, _>(name.as_str())
        .load::<IpAddressAssignmentRow>(connection)?;
        rows.into_iter()
            .map(IpAddressAssignmentRow::into_domain)
            .collect()
    }

    pub(super) fn query_ip_address(
        connection: &mut PgConnection,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        sql_query(
            "SELECT ip.id,
                    ip.host_id,
                    ip.attachment_id,
                    host(ip.address) AS address,
                    ip.family::int AS family,
                    nw.id AS network_id,
                    ip.mac_address,
                    ip.created_at,
                    ip.updated_at
             FROM ip_addresses ip
             JOIN LATERAL (
               SELECT id
               FROM networks
               WHERE ip.address <<= network
               ORDER BY masklen(network) DESC
               LIMIT 1
             ) nw ON true
             WHERE host(ip.address) = $1",
        )
        .bind::<Text, _>(address.as_str())
        .get_result::<IpAddressAssignmentRow>(connection)
        .optional()?
        .ok_or_else(|| AppError::not_found(format!("IP address {}", address.as_str())))?
        .into_domain()
    }

    pub(super) fn assign_ip_address_tx(
        connection: &mut PgConnection,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        let host = Self::query_host_by_name(connection, command.host_name())?;

        let (network, address) = if let Some(address) = command.address().cloned() {
            let network = Self::query_network_containing_ip(connection, &address)?;
            Self::ensure_address_usable(connection, &network, &address)?;
            (network, address)
        } else {
            let target_network = command.network().ok_or_else(|| {
                AppError::validation("automatic allocation requires a target network")
            })?;
            let network = Self::query_network_by_cidr(connection, target_network)?;
            let address = Self::allocate_address_in_network(connection, &network)?;
            (network, address)
        };

        let family = match address.as_inner() {
            std::net::IpAddr::V4(_) => 4,
            std::net::IpAddr::V6(_) => 6,
        };
        let attachment = Self::find_or_create_attachment(
            connection,
            host.name(),
            network.cidr(),
            command.mac_address(),
        )?;

        let assignment = sql_query(
            "INSERT INTO ip_addresses (host_id, attachment_id, address, family, mac_address)
             VALUES ($1, $2, $3::inet, $4, $5)
             RETURNING id, host_id, attachment_id, host(address) AS address, family::int AS family, $6 AS network_id,
                       mac_address, created_at, updated_at",
        )
        .bind::<SqlUuid, _>(host.id())
        .bind::<SqlUuid, _>(attachment.id())
        .bind::<Text, _>(address.as_str())
        .bind::<Integer, _>(family)
        .bind::<Nullable<Text>, _>(attachment.mac_address().map(|value| value.as_str()))
        .bind::<SqlUuid, _>(network.id())
        .get_result::<IpAddressAssignmentRow>(connection)
        .map_err(map_unique("IP address is already allocated"))?
        .into_domain()?;

        // Auto-create DHCP identifiers from MAC address
        if let Some(mac) = attachment.mac_address() {
            if assignment.family() == 4 && command.auto_v4_client_id() {
                let existing =
                    Self::list_attachment_dhcp_identifiers_tx(connection, attachment.id())?;
                if !existing.iter().any(|id| id.family().as_u8() == 4) {
                    let client_id_value = format!("01:{}", mac.as_str());
                    Self::create_attachment_dhcp_identifier_tx(
                        connection,
                        CreateAttachmentDhcpIdentifier::new(
                            attachment.id(),
                            DhcpIdentifierFamily::V4,
                            DhcpIdentifierKind::ClientId,
                            client_id_value,
                            DhcpPriority::new(1000),
                        )?,
                    )?;
                }
            }
            if assignment.family() == 6 && command.auto_v6_duid_ll() {
                let existing =
                    Self::list_attachment_dhcp_identifiers_tx(connection, attachment.id())?;
                if !existing.iter().any(|id| id.family().as_u8() == 6) {
                    let duid_ll_value = format!("00:03:00:01:{}", mac.as_str());
                    Self::create_attachment_dhcp_identifier_tx(
                        connection,
                        CreateAttachmentDhcpIdentifier::new(
                            attachment.id(),
                            DhcpIdentifierFamily::V6,
                            DhcpIdentifierKind::DuidLl,
                            duid_ll_value,
                            DhcpPriority::new(1000),
                        )?,
                    )?;
                }
            }
        }

        Ok(assignment)
    }

    /// Shared logic for auto-creating a DNS record (A/AAAA or PTR) when an IP is assigned.
    ///
    /// The caller provides the record type name, the owner name for the RRSet, the JSON
    /// data payload, and how to build the `CreateRecordInstance` command (anchored for
    /// forward records, unanchored for PTR records).
    pub(in crate::storage::postgres) fn auto_create_record(
        connection: &mut PgConnection,
        type_name: &str,
        owner_name_str: &str,
        data: serde_json::Value,
        build_command: impl FnOnce(
            &str,
            serde_json::Value,
        ) -> Result<
            crate::domain::resource_records::CreateRecordInstance,
            AppError,
        >,
    ) -> Result<(), AppError> {
        use crate::domain::types::{DnsName, RecordTypeName};

        let record_type =
            Self::query_record_type_by_name(connection, &RecordTypeName::new(type_name)?)?;

        let owner_name = DnsName::new(owner_name_str)?;

        let command = build_command(type_name, data)?;

        let (anchor_id, _anchor_name, zone_id) = Self::resolve_record_owner(
            connection,
            command.owner_kind(),
            command.anchor_name(),
            command.owner_name(),
        )?;

        let validated = record_type.validate_record_input(
            command.owner_name(),
            command.data(),
            command.raw_rdata(),
        )?;

        let same_owner_records =
            Self::query_existing_owner_records(connection, command.owner_name())?;

        let existing_rrset =
            Self::query_rrset_by_type_and_owner(connection, record_type.id(), &owner_name)?;

        let same_rrset_records = if let Some(rrset) = &existing_rrset {
            Self::query_existing_rrset_records(connection, rrset.id())?
        } else {
            Vec::new()
        };

        let alias_lookup = match &validated {
            crate::domain::resource_records::ValidatedRecordContent::Structured(normalized) => {
                Self::query_alias_owner_names(
                    connection,
                    &crate::domain::resource_records::alias_target_names(
                        normalized,
                        record_type.name(),
                    ),
                )?
            }
            crate::domain::resource_records::ValidatedRecordContent::RawRdata(_) => BTreeMap::new(),
        };
        let alias_owner_names = alias_lookup
            .into_iter()
            .filter_map(|(name, is_alias)| is_alias.then_some(name))
            .collect::<std::collections::BTreeSet<_>>();

        crate::domain::resource_records::validate_record_relationships(
            &record_type,
            command.ttl(),
            &validated,
            &same_owner_records,
            &same_rrset_records,
            &alias_owner_names,
        )?;

        let rrset = if let Some(rrset) = existing_rrset {
            rrset
        } else {
            Self::insert_rrset(connection, &record_type, &command, anchor_id, zone_id)?
        };

        let rendered = if let crate::domain::resource_records::ValidatedRecordContent::Structured(
            ref normalized,
        ) = validated
        {
            Self::render_record_data(record_type.schema().render_template(), normalized)?
        } else {
            None
        };

        let record = Self::insert_record(connection, &rrset, rendered, &validated)?;

        if let Some(zone_id) = record.zone_id() {
            Self::bump_zone_serial_tx(connection, zone_id);
        }

        Ok(())
    }

    /// Auto-create an A or AAAA record for a newly assigned IP address.
    fn auto_create_forward_record(
        connection: &mut PgConnection,
        assignment: &IpAddressAssignment,
    ) -> Result<(), AppError> {
        use crate::domain::resource_records::{CreateRecordInstance, RecordOwnerKind};
        use crate::domain::types::RecordTypeName;

        let host_name = hosts::table
            .filter(hosts::id.eq(assignment.host_id()))
            .select(hosts::name)
            .first::<String>(connection)
            .optional()?;

        let Some(host_name) = host_name else {
            return Ok(());
        };

        let type_name = match assignment.family() {
            4 => "A",
            6 => "AAAA",
            _ => return Ok(()),
        };

        let data = serde_json::json!({ "address": assignment.address().as_str() });

        Self::auto_create_record(connection, type_name, &host_name, data, |tn, d| {
            CreateRecordInstance::new(
                RecordTypeName::new(tn)?,
                RecordOwnerKind::Host,
                &host_name,
                None,
                d,
            )
        })
    }

    /// Auto-create a PTR record in the matching reverse zone for a newly assigned IP.
    fn auto_create_ptr_record(
        connection: &mut PgConnection,
        assignment: &IpAddressAssignment,
    ) -> Result<(), AppError> {
        use crate::domain::resource_records::CreateRecordInstance;
        use crate::domain::types::{DnsName, RecordTypeName, ip_to_ptr_name};

        let host_name = hosts::table
            .filter(hosts::id.eq(assignment.host_id()))
            .select(hosts::name)
            .first::<String>(connection)
            .optional()?;

        let Some(host_name) = host_name else {
            return Ok(());
        };

        let ptr_name = ip_to_ptr_name(assignment.address());

        // Bail out early if there is no reverse zone for this IP.
        let reverse_zone_id =
            Self::best_matching_zone_for_owner_name(connection, &DnsName::new(&ptr_name)?)?;
        if reverse_zone_id.is_none() {
            return Ok(());
        }

        let target = if host_name.ends_with('.') {
            host_name
        } else {
            format!("{}.", host_name)
        };

        let data = serde_json::json!({ "ptrdname": target });

        Self::auto_create_record(connection, "PTR", &ptr_name, data, |tn, d| {
            CreateRecordInstance::new_unanchored(RecordTypeName::new(tn)?, &ptr_name, None, d)
        })
    }

    /// Shared logic for auto-deleting a DNS record by owner name, record type,
    /// and an optional data filter (e.g. matching a specific address value).
    fn auto_delete_record(
        connection: &mut PgConnection,
        owner_name: &str,
        type_name: &str,
        data_filter: Option<(&str, &str)>,
    ) -> Result<(), AppError> {
        let deleted = if let Some((json_key, json_value)) = data_filter {
            sql_query(
                "DELETE FROM records r
                 USING rrsets rs, record_types rt
                 WHERE r.rrset_id = rs.id AND rs.type_id = rt.id
                 AND rs.owner_name = $1 AND rt.name = $2
                 AND r.data->>$3 = $4",
            )
            .bind::<Text, _>(owner_name)
            .bind::<Text, _>(type_name)
            .bind::<Text, _>(json_key)
            .bind::<Text, _>(json_value)
            .execute(connection)?
        } else {
            sql_query(
                "DELETE FROM records r
                 USING rrsets rs, record_types rt
                 WHERE r.rrset_id = rs.id AND rs.type_id = rt.id
                 AND rs.owner_name = $1 AND rt.name = $2",
            )
            .bind::<Text, _>(owner_name)
            .bind::<Text, _>(type_name)
            .execute(connection)?
        };

        if deleted > 0 {
            sql_query(
                "DELETE FROM rrsets WHERE NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = rrsets.id)",
            )
            .execute(connection)?;
        }

        Ok(())
    }

    /// Delete the A/AAAA record matching the unassigned IP.
    fn auto_delete_forward_record(
        connection: &mut PgConnection,
        row: &IpAddressAssignmentRow,
    ) -> Result<(), AppError> {
        let type_name = match row.family() {
            4 => "A",
            6 => "AAAA",
            _ => return Ok(()),
        };

        let host_name = hosts::table
            .filter(hosts::id.eq(row.host_id()))
            .select(hosts::name)
            .first::<String>(connection)
            .optional()?;

        let Some(host_name) = host_name else {
            return Ok(());
        };

        let addr = row.address_str();
        Self::auto_delete_record(connection, &host_name, type_name, Some(("address", &addr)))
    }

    /// Delete the PTR record matching the unassigned IP.
    fn auto_delete_ptr_record(
        connection: &mut PgConnection,
        row: &IpAddressAssignmentRow,
    ) -> Result<(), AppError> {
        use crate::domain::types::{IpAddressValue, ip_to_ptr_name};

        let address = IpAddressValue::new(row.address_str())?;
        let ptr_name = ip_to_ptr_name(&address);

        Self::auto_delete_record(connection, &ptr_name, "PTR", None)
    }

    /// Resolve updated field values from an `UpdateHost` command, falling back
    /// to the existing host values for any field not present in the command.
    /// Also looks up the zone UUID when a zone name is provided.
    fn resolve_host_update_values(
        connection: &mut PgConnection,
        old_host: &Host,
        command: &UpdateHost,
    ) -> Result<ResolvedHostUpdate, AppError> {
        let name = command
            .name
            .as_ref()
            .map(|v| v.as_str().to_string())
            .unwrap_or_else(|| old_host.name().as_str().to_string());
        let ttl: Option<i32> = command
            .ttl
            .map(|t| t.as_i32())
            .resolve(old_host.ttl().map(|t| t.as_i32()));
        let comment = command
            .comment
            .as_ref()
            .cloned()
            .unwrap_or_else(|| old_host.comment().to_string());
        let zone_name: Option<String> = command
            .zone
            .clone()
            .map(|z| z.as_str().to_string())
            .resolve(old_host.zone().map(|z| z.as_str().to_string()));
        let zone_id: Option<uuid::Uuid> = match &zone_name {
            Some(zn) => Some(
                forward_zones::table
                    .filter(forward_zones::name.eq(zn))
                    .select(forward_zones::id)
                    .first::<uuid::Uuid>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("forward zone '{}' was not found", zn))
                    })?,
            ),
            None => None,
        };
        Ok(ResolvedHostUpdate {
            name,
            ttl,
            comment,
            zone_name,
            zone_id,
        })
    }

    /// When a host is renamed, cascade the new name to all rrsets and records
    /// owned by that host, and bump the zone serial.
    fn cascade_host_rename(
        connection: &mut PgConnection,
        host_id: uuid::Uuid,
        new_name: &str,
        zone_id: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        use crate::db::schema::{records, rrsets};

        diesel::update(rrsets::table.filter(rrsets::anchor_id.eq(host_id)))
            .set((
                rrsets::owner_name.eq(new_name),
                rrsets::anchor_name.eq(new_name),
            ))
            .execute(connection)?;
        diesel::update(records::table.filter(records::owner_id.eq(host_id)))
            .set(records::owner_name.eq(new_name))
            .execute(connection)?;
        if let Some(zone_id) = zone_id {
            Self::bump_zone_serial_tx(connection, zone_id);
        }
        Ok(())
    }
}

#[async_trait]
impl HostStore for PostgresStorage {
    async fn list_hosts(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |c| {
                let base = "SELECT h.id, h.name::text AS name, fz.name::text AS zone_name, \
                        h.ttl, h.comment, h.created_at, h.updated_at \
                        FROM hosts h LEFT JOIN forward_zones fz ON fz.id = h.zone_id";

                let (clauses, values) = filter.sql_conditions();
                let where_str = if clauses.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", clauses.join(" AND "))
                };
                let order_col = match page.sort_by() {
                    Some("comment") => "h.comment",
                    Some("created_at") => "h.created_at",
                    Some("updated_at") => "h.updated_at",
                    _ => "h.name::text",
                };
                let order_dir = match page.sort_direction() {
                    crate::domain::pagination::SortDirection::Asc => "ASC",
                    crate::domain::pagination::SortDirection::Desc => "DESC",
                };
                let query_str = format!("{base}{where_str} ORDER BY {order_col} {order_dir}, h.id");

                let rows = run_dynamic_query::<HostRow>(c, &query_str, &values)?;
                let items: Vec<Host> = rows
                    .into_iter()
                    .map(|row| row.into_domain())
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn create_host(&self, command: CreateHost) -> Result<Host, AppError> {
        let name = command.name().as_str().to_string();
        let zone_name = command.zone().map(|zone| zone.as_str().to_string());
        let ttl = command.ttl().map(|ttl| ttl.as_i32());
        let comment = command.comment().to_string();
        let ip_specs = command.ip_assignments().to_vec();
        self.database
            .run(move |connection| {
                connection.transaction::<Host, AppError, _>(|connection| {
                    let zone_id = match zone_name.as_ref() {
                        Some(zone_name) => Some(
                            forward_zones::table
                                .filter(forward_zones::name.eq(zone_name))
                                .select(forward_zones::id)
                                .first::<uuid::Uuid>(connection)
                                .optional()?
                                .ok_or_else(|| {
                                    AppError::not_found(format!(
                                        "forward zone '{}' was not found",
                                        zone_name
                                    ))
                                })?,
                        ),
                        None => None,
                    };
                    let host = sql_query(
                        "INSERT INTO hosts (name, zone_id, ttl, comment)
                         VALUES ($1, $2, $3, $4)
                         RETURNING id, name::text AS name, $5::text AS zone_name, ttl, comment, created_at, updated_at",
                    )
                    .bind::<Text, _>(&name)
                    .bind::<Nullable<SqlUuid>, _>(zone_id)
                    .bind::<Nullable<Integer>, _>(ttl)
                    .bind::<Text, _>(&comment)
                    .bind::<Nullable<Text>, _>(zone_name.as_deref())
                    .get_result::<HostRow>(connection)
                    .map_err(map_unique("host already exists"))?
                    .into_domain()?;

                    // Process IP assignments atomically within the transaction
                    for spec in ip_specs {
                        let assign_cmd =
                            if *spec.allocation() == AllocationPolicy::Random {
                                if let Some(network_cidr) = spec.network() {
                                    let network =
                                        Self::query_network_by_cidr(connection, network_cidr)?;
                                    let address = Self::allocate_random_address_in_network(
                                        connection, &network,
                                    )?;
                                    let cmd = AssignIpAddress::new(
                                        host.name().clone(),
                                        Some(address),
                                        None,
                                        spec.mac_address().cloned(),
                                    )?;
                                    cmd.with_auto_dhcp(
                                        spec.auto_v4_client_id(),
                                        spec.auto_v6_duid_ll(),
                                    )
                                } else {
                                    spec.into_assign_command(host.name().clone())?
                                }
                            } else {
                                spec.into_assign_command(host.name().clone())?
                            };
                        let assignment = Self::assign_ip_address_tx(connection, assign_cmd)?;
                        Self::auto_create_forward_record(connection, &assignment)?;
                        Self::auto_create_ptr_record(connection, &assignment)?;
                    }

                    Ok(host)
                })
            })
            .await
    }

    async fn get_host_by_name(&self, name: &Hostname) -> Result<Host, AppError> {
        let name = name.clone();
        self.database
            .run(move |connection| Self::query_host_by_name(connection, &name))
            .await
    }

    async fn get_host_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError> {
        let name = name.clone();
        self.database
            .run(move |connection| Self::query_host_auth_context(connection, &name))
            .await
    }

    async fn update_host(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError> {
        let name = name.clone();
        self.database
            .run(move |connection| {
                connection.transaction::<Host, AppError, _>(|connection| {
                    let old_host = Self::query_host_by_name(connection, &name)?;
                    let resolved =
                        Self::resolve_host_update_values(connection, &old_host, &command)?;

                    let host = sql_query(
                        "UPDATE hosts
                         SET name = $1, ttl = $2, comment = $3,
                             zone_id = $4, updated_at = now()
                         WHERE id = $5
                         RETURNING id, name::text AS name, $6::text AS zone_name, ttl, comment, created_at, updated_at",
                    )
                    .bind::<Text, _>(&resolved.name)
                    .bind::<Nullable<Integer>, _>(resolved.ttl)
                    .bind::<Text, _>(&resolved.comment)
                    .bind::<Nullable<SqlUuid>, _>(resolved.zone_id)
                    .bind::<SqlUuid, _>(old_host.id())
                    .bind::<Nullable<Text>, _>(resolved.zone_name.as_deref())
                    .get_result::<HostRow>(connection)
                    .map_err(map_unique("host already exists"))?
                    .into_domain()?;

                    if host.name() != old_host.name() {
                        Self::cascade_host_rename(
                            connection,
                            old_host.id(),
                            &resolved.name,
                            resolved.zone_id,
                        )?;
                    }

                    Ok(host)
                })
            })
            .await
    }

    async fn delete_host(&self, name: &Hostname) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| {
                connection.transaction::<(), AppError, _>(|connection| {
                    use crate::db::schema::records;

                    // Look up the host id and zone_id
                    let (host_id, host_zone_id) = hosts::table
                        .filter(hosts::name.eq(&name))
                        .select((hosts::id, hosts::zone_id))
                        .first::<(uuid::Uuid, Option<uuid::Uuid>)>(connection)
                        .optional()?
                        .ok_or_else(|| AppError::not_found(format!("host '{}' was not found", name)))?;

                    // Cascade: delete all records owned by this host
                    diesel::delete(records::table.filter(records::owner_id.eq(host_id)))
                        .execute(connection)?;
                    sql_query(
                        "DELETE FROM rrsets WHERE NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = rrsets.id)",
                    )
                    .execute(connection)?;

                    // Cascade: bump zone serial for the host's zone
                    if let Some(zone_id) = host_zone_id {
                        Self::bump_zone_serial_tx(connection, zone_id);
                    }

                    // Delete the host (CASCADE handles ip_addresses via FK)
                    diesel::delete(hosts::table.filter(hosts::id.eq(host_id)))
                        .execute(connection)?;

                    Ok(())
                })
            })
            .await
    }

    async fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_ip_addresses(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn list_ip_addresses_for_host(
        &self,
        host: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        let host = host.clone();
        let page = page.clone();
        self.database
            .run(move |connection| {
                let items = Self::query_ip_addresses_for_host(connection, &host)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn get_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        let address = *address;
        self.database
            .run(move |connection| Self::query_ip_address(connection, &address))
            .await
    }

    async fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction(|connection| {
                    let assignment = Self::assign_ip_address_tx(connection, command)?;

                    // Auto-create A/AAAA record for the host
                    Self::auto_create_forward_record(connection, &assignment)?;

                    // Auto-create PTR record in the matching reverse zone (if one exists)
                    Self::auto_create_ptr_record(connection, &assignment)?;

                    Ok(assignment)
                })
            })
            .await
    }

    async fn update_ip_address(
        &self,
        address: &crate::domain::types::IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        let addr = address.as_str();
        if command.mac_address.is_unchanged() {
            return self.database.run(move |connection| {
                let row = sql_query(
                    "SELECT ip.id, ip.host_id, ip.attachment_id, host(ip.address) AS address, ip.family::int AS family, \
                     nw.id AS network_id, ip.mac_address, ip.created_at, ip.updated_at \
                     FROM ip_addresses ip \
                     JOIN LATERAL ( \
                       SELECT id FROM networks WHERE ip.address <<= network ORDER BY masklen(network) DESC LIMIT 1 \
                     ) nw ON true \
                     WHERE ip.address = $1::inet",
                )
                .bind::<Text, _>(&addr)
                .get_result::<IpAddressAssignmentRow>(connection)
                .map_err(|_| AppError::not_found(format!("IP address assignment '{}' was not found", addr)))?;
                row.into_domain()
            }).await;
        }
        let mac_str: Option<String> = command
            .mac_address
            .into_set()
            .map(|m| m.as_str().to_string());
        self.database
            .run(move |connection| {
                connection.transaction::<IpAddressAssignment, AppError, _>(|connection| {
                    let existing = sql_query(
                        "SELECT ip.id, ip.host_id, ip.attachment_id, host(ip.address) AS address, ip.family::int AS family, \
                         nw.id AS network_id, ip.mac_address, ip.created_at, ip.updated_at \
                         FROM ip_addresses ip \
                         JOIN LATERAL ( \
                           SELECT id FROM networks WHERE ip.address <<= network ORDER BY masklen(network) DESC LIMIT 1 \
                         ) nw ON true \
                         WHERE ip.address = $1::inet",
                    )
                    .bind::<Text, _>(&addr)
                    .get_result::<IpAddressAssignmentRow>(connection)
                    .map_err(|_| AppError::not_found(format!("IP address assignment '{}' was not found", addr)))?
                    .into_domain()?;

                    let host = hosts::table
                        .filter(hosts::id.eq(existing.host_id()))
                        .select(hosts::name)
                        .first::<String>(connection)
                        .optional()?
                        .ok_or_else(|| AppError::not_found("host for IP address assignment was not found"))?;
                    let network = Self::query_network_by_id(connection, existing.network_id())?;
                    let attachment = Self::find_or_create_attachment(
                        connection,
                        &Hostname::new(host)?,
                        network.cidr(),
                        mac_str.as_deref().map(MacAddressValue::new).transpose()?.as_ref(),
                    )?;

                    sql_query(
                        "UPDATE ip_addresses SET attachment_id = $1, mac_address = $2, updated_at = now() \
                         WHERE address = $3::inet",
                    )
                    .bind::<SqlUuid, _>(attachment.id())
                    .bind::<Nullable<Text>, _>(mac_str.as_deref())
                    .bind::<Text, _>(&addr)
                    .execute(connection)?;

                    sql_query(
                        "SELECT ip.id, ip.host_id, ip.attachment_id, host(ip.address) AS address, ip.family::int AS family, \
                         nw.id AS network_id, ip.mac_address, ip.created_at, ip.updated_at \
                         FROM ip_addresses ip \
                         JOIN LATERAL ( \
                           SELECT id FROM networks WHERE ip.address <<= network ORDER BY masklen(network) DESC LIMIT 1 \
                         ) nw ON true \
                         WHERE ip.address = $1::inet",
                    )
                    .bind::<Text, _>(&addr)
                    .get_result::<IpAddressAssignmentRow>(connection)
                    .map_err(|_| AppError::not_found(format!("IP address assignment '{}' was not found", addr)))?
                    .into_domain()
                })
            })
            .await
    }

    async fn unassign_ip_address(
        &self,
        address: &crate::domain::types::IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        let addr = address.as_str();
        self.database
            .run(move |connection| {
                connection.transaction::<IpAddressAssignment, AppError, _>(|connection| {
                    // First query the assignment so we can return it
                    let row = sql_query(
                        "SELECT ip.id,
                                ip.host_id,
                                ip.attachment_id,
                                host(ip.address) AS address,
                                ip.family::int AS family,
                                nw.id AS network_id,
                                ip.mac_address,
                                ip.created_at,
                                ip.updated_at
                         FROM ip_addresses ip
                         JOIN LATERAL (
                           SELECT id
                           FROM networks
                           WHERE ip.address <<= network
                           ORDER BY masklen(network) DESC
                           LIMIT 1
                         ) nw ON true
                         WHERE ip.address = $1::inet",
                    )
                    .bind::<Text, _>(&addr)
                    .get_result::<IpAddressAssignmentRow>(connection)
                    .optional()?;

                    let row = row.ok_or_else(|| {
                        AppError::not_found(format!(
                            "IP address assignment '{}' was not found",
                            addr
                        ))
                    })?;

                    let deleted = sql_query("DELETE FROM ip_addresses WHERE address = $1::inet")
                        .bind::<Text, _>(&addr)
                        .execute(connection)?;

                    if deleted == 0 {
                        return Err(AppError::not_found(format!(
                            "IP address assignment '{}' was not found",
                            addr
                        )));
                    }

                    // Cascade: delete matching A/AAAA record
                    Self::auto_delete_forward_record(connection, &row)?;

                    // Cascade: delete matching PTR record
                    Self::auto_delete_ptr_record(connection, &row)?;

                    row.into_domain()
                })
            })
            .await
    }
}
