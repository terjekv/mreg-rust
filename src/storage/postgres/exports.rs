use std::collections::HashMap;

use async_trait::async_trait;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl,
    SelectableHelper, insert_into, sql_query, sql_types::Uuid as SqlUuid, update,
};
use minijinja::Environment;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::{
        models::{AttachmentCommunityAssignmentRow, ExportRunRow, ExportTemplateRow},
        schema::{export_runs, export_templates},
    },
    domain::{
        exports::{CreateExportRun, CreateExportTemplate, ExportRun, ExportTemplate},
        pagination::{Page, PageRequest},
        tasks::CreateTask,
    },
    errors::AppError,
    storage::ExportStore,
};

use super::PostgresStorage;
use super::helpers::{map_unique, vec_to_page};

fn dhcp_identifier_kind_name(kind: crate::domain::attachment::DhcpIdentifierKind) -> &'static str {
    match kind {
        crate::domain::attachment::DhcpIdentifierKind::ClientId => "client_id",
        crate::domain::attachment::DhcpIdentifierKind::DuidLlt => "duid_llt",
        crate::domain::attachment::DhcpIdentifierKind::DuidEn => "duid_en",
        crate::domain::attachment::DhcpIdentifierKind::DuidLl => "duid_ll",
        crate::domain::attachment::DhcpIdentifierKind::DuidUuid => "duid_uuid",
        crate::domain::attachment::DhcpIdentifierKind::DuidRaw => "duid_raw",
    }
}

fn render_export_template(template: &ExportTemplate, context: &Value) -> Result<String, AppError> {
    match template.engine() {
        "json" => serde_json::to_string_pretty(context).map_err(AppError::internal),
        "minijinja" => {
            let mut env = Environment::new();
            env.add_template("export", template.body())
                .map_err(AppError::internal)?;
            env.get_template("export")
                .map_err(AppError::internal)?
                .render(minijinja::value::Value::from_serialize(context))
                .map_err(AppError::internal)
        }
        other => Err(AppError::validation(format!(
            "unsupported export template engine '{other}'"
        ))),
    }
}

impl PostgresStorage {
    fn query_export_templates(
        connection: &mut PgConnection,
    ) -> Result<Vec<ExportTemplate>, AppError> {
        let rows = export_templates::table
            .select(ExportTemplateRow::as_select())
            .order(export_templates::name.asc())
            .load::<ExportTemplateRow>(connection)?;
        rows.into_iter()
            .map(ExportTemplateRow::into_domain)
            .collect()
    }

    fn query_export_runs(connection: &mut PgConnection) -> Result<Vec<ExportRun>, AppError> {
        let rows = export_runs::table
            .select(ExportRunRow::as_select())
            .order(export_runs::created_at.desc())
            .load::<ExportRunRow>(connection)?;
        rows.into_iter().map(ExportRunRow::into_domain).collect()
    }

    fn zone_export_context(
        connection: &mut PgConnection,
        run: &ExportRun,
        zone_name: &str,
    ) -> Result<Value, AppError> {
        let is_forward = run.scope() == "forward_zone";

        // Look up the specific zone by name (single-row query, not load-all)
        let zone_json: Value;
        let zone_id: Uuid;

        if is_forward {
            let zone = Self::get_forward_zone_by_name_impl(connection, zone_name)?;
            zone_id = zone.id();
            zone_json = json!({
                "name": zone.name().as_str(),
                "primary_ns": zone.primary_ns().as_str(),
                "nameservers": zone.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
                "email": zone.email().as_str(),
                "serial_no": zone.serial_no().as_u64(),
                "refresh": zone.refresh(),
                "retry": zone.retry(),
                "expire": zone.expire(),
                "soa_ttl": zone.soa_ttl().as_u32(),
                "default_ttl": zone.default_ttl().as_u32(),
                "updated": zone.updated(),
            });
        } else {
            let zone = Self::get_reverse_zone_by_name_impl(connection, zone_name)?;
            zone_id = zone.id();
            zone_json = json!({
                "name": zone.name().as_str(),
                "network": zone.network().map(|v| v.as_str()),
                "primary_ns": zone.primary_ns().as_str(),
                "nameservers": zone.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
                "email": zone.email().as_str(),
                "serial_no": zone.serial_no().as_u64(),
                "refresh": zone.refresh(),
                "retry": zone.retry(),
                "expire": zone.expire(),
                "soa_ttl": zone.soa_ttl().as_u32(),
                "default_ttl": zone.default_ttl().as_u32(),
                "updated": zone.updated(),
            });
        }

        // Query records scoped to this zone (not load-all)
        let record_types = Self::query_record_types(connection)?;
        let zone_records = Self::query_records_for_zone(connection, zone_id)?;
        let records: Vec<Value> = zone_records
            .iter()
            .map(|record| {
                let dns_type = record_types
                    .iter()
                    .find(|rt| rt.name() == record.type_name())
                    .and_then(|rt| rt.dns_type());
                let rfc3597_rendered = record.raw_rdata().map(|raw| {
                    if let Some(type_num) = dns_type {
                        format!("TYPE{} {}", type_num, raw.presentation())
                    } else {
                        format!("{} {}", record.type_name().as_str(), raw.presentation())
                    }
                });
                json!({
                    "owner_name": record.owner_name(),
                    "type_name": record.type_name().as_str(),
                    "ttl": record.ttl().map(|ttl| ttl.as_u32()),
                    "data": record.data(),
                    "rendered": record.rendered().map(str::to_string).or(rfc3597_rendered),
                })
            })
            .collect();

        // Query delegations for this zone with bulk-loaded nameservers
        let delegations: Vec<Value> = if is_forward {
            use crate::db::models::ForwardDelegationRow;
            use crate::db::schema::nameservers as ns_table;
            use crate::db::schema::{
                forward_zone_delegation_nameservers, forward_zone_delegations,
            };

            let del_rows = forward_zone_delegations::table
                .filter(forward_zone_delegations::zone_id.eq(zone_id))
                .select(ForwardDelegationRow::as_select())
                .order(forward_zone_delegations::name.asc())
                .load::<ForwardDelegationRow>(connection)?;

            let del_ids: Vec<Uuid> = del_rows.iter().map(|r| r.id()).collect();
            let ns_pairs = forward_zone_delegation_nameservers::table
                .inner_join(ns_table::table)
                .filter(forward_zone_delegation_nameservers::delegation_id.eq_any(&del_ids))
                .select((
                    forward_zone_delegation_nameservers::delegation_id,
                    ns_table::name,
                ))
                .order(ns_table::name.asc())
                .load::<(Uuid, String)>(connection)?;
            let mut ns_map: HashMap<Uuid, Vec<String>> = HashMap::new();
            for (delegation_id, name) in ns_pairs {
                ns_map.entry(delegation_id).or_default().push(name);
            }

            del_rows
                .into_iter()
                .map(|row| {
                    let ns = ns_map.remove(&row.id()).unwrap_or_default();
                    let ns_names: Vec<crate::domain::types::DnsName> = ns
                        .iter()
                        .map(crate::domain::types::DnsName::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    let delegation = row.into_forward_delegation(ns_names)?;
                    Ok(json!({
                        "name": delegation.name().as_str(),
                        "nameservers": ns,
                        "comment": delegation.comment(),
                    }))
                })
                .collect::<Result<Vec<_>, AppError>>()?
        } else {
            use crate::db::models::ReverseDelegationRow;
            use crate::db::schema::nameservers as ns_table;
            use crate::db::schema::{
                reverse_zone_delegation_nameservers, reverse_zone_delegations,
            };

            let del_rows = reverse_zone_delegations::table
                .filter(reverse_zone_delegations::zone_id.eq(zone_id))
                .select(ReverseDelegationRow::as_select())
                .order(reverse_zone_delegations::name.asc())
                .load::<ReverseDelegationRow>(connection)?;

            let del_ids: Vec<Uuid> = del_rows.iter().map(|r| r.id()).collect();
            let ns_pairs = reverse_zone_delegation_nameservers::table
                .inner_join(ns_table::table)
                .filter(reverse_zone_delegation_nameservers::delegation_id.eq_any(&del_ids))
                .select((
                    reverse_zone_delegation_nameservers::delegation_id,
                    ns_table::name,
                ))
                .order(ns_table::name.asc())
                .load::<(Uuid, String)>(connection)?;
            let mut ns_map: HashMap<Uuid, Vec<String>> = HashMap::new();
            for (delegation_id, name) in ns_pairs {
                ns_map.entry(delegation_id).or_default().push(name);
            }

            del_rows
                .into_iter()
                .map(|row| {
                    let ns = ns_map.remove(&row.id()).unwrap_or_default();
                    let ns_names: Vec<crate::domain::types::DnsName> = ns
                        .iter()
                        .map(crate::domain::types::DnsName::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    let delegation = row.into_reverse_delegation(ns_names)?;
                    Ok(json!({
                        "name": delegation.name().as_str(),
                        "nameservers": ns,
                        "comment": delegation.comment(),
                    }))
                })
                .collect::<Result<Vec<_>, AppError>>()?
        };

        // Query hosts and IPs scoped to this zone (forward only)
        let (hosts, ip_addresses) = if is_forward {
            let zone_hosts = Self::query_hosts_for_zone(connection, zone_id)?;
            let host_json: Vec<Value> = zone_hosts
                .iter()
                .map(|host| {
                    json!({
                        "id": host.id().to_string(),
                        "name": host.name().as_str(),
                        "ttl": host.ttl().map(|ttl| ttl.as_u32()),
                        "comment": host.comment(),
                    })
                })
                .collect();

            let host_ids: Vec<Uuid> = zone_hosts.iter().map(|h| h.id()).collect();
            let zone_ips = Self::query_ip_addresses_for_hosts(connection, &host_ids)?;
            let ip_json: Vec<Value> = zone_ips
                .iter()
                .map(|assignment| {
                    json!({
                        "host_id": assignment.host_id().to_string(),
                        "address": assignment.address().as_str(),
                        "family": assignment.family(),
                        "mac_address": assignment.mac_address().map(|m| m.as_str()),
                    })
                })
                .collect();

            (host_json, ip_json)
        } else {
            (vec![], vec![])
        };

        Ok(json!({
            "zone": zone_json,
            "delegations": delegations,
            "records": records,
            "hosts": hosts,
            "ip_addresses": ip_addresses,
            "scope": run.scope(),
            "parameters": run.parameters(),
        }))
    }

    fn export_context(connection: &mut PgConnection, run: &ExportRun) -> Result<Value, AppError> {
        let labels = Self::query_labels(connection)?;
        let nameservers = Self::query_nameservers(connection)?;
        let forward_zones = Self::query_forward_zones(connection)?;
        let reverse_zones = Self::query_reverse_zones(connection)?;
        let networks = Self::query_networks(connection)?;
        let hosts = Self::query_hosts(connection)?;
        let ip_addresses = Self::query_ip_addresses(connection)?;
        let record_types = Self::query_record_types(connection)?;
        let rrsets = Self::query_rrsets(connection)?;
        let records = Self::query_records(connection)?;

        // Build forward zone delegation data
        let forward_zone_delegations: Vec<Value> = {
            use crate::db::models::ForwardDelegationRow;
            use crate::db::schema::nameservers as ns_table;
            use crate::db::schema::{
                forward_zone_delegation_nameservers, forward_zone_delegations,
            };

            // Bulk-load all forward delegation-nameserver pairs in one query
            let fwd_del_ns_pairs = forward_zone_delegation_nameservers::table
                .inner_join(ns_table::table)
                .select((
                    forward_zone_delegation_nameservers::delegation_id,
                    ns_table::name,
                ))
                .order(ns_table::name.asc())
                .load::<(Uuid, String)>(connection)?;
            let mut fwd_del_ns_map: HashMap<Uuid, Vec<String>> = HashMap::new();
            for (delegation_id, name) in fwd_del_ns_pairs {
                fwd_del_ns_map.entry(delegation_id).or_default().push(name);
            }

            let rows = forward_zone_delegations::table
                .select(ForwardDelegationRow::as_select())
                .order(forward_zone_delegations::name.asc())
                .load::<ForwardDelegationRow>(connection)?;
            rows.into_iter()
                .map(|row| {
                    let ns = fwd_del_ns_map.remove(&row.id()).unwrap_or_default();
                    let ns_names: Vec<crate::domain::types::DnsName> = ns
                        .iter()
                        .map(crate::domain::types::DnsName::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    let delegation = row.into_forward_delegation(ns_names)?;
                    Ok(json!({
                        "name": delegation.name().as_str(),
                        "zone_id": delegation.zone_id().to_string(),
                        "comment": delegation.comment(),
                        "nameservers": ns,
                    }))
                })
                .collect::<Result<Vec<_>, AppError>>()?
        };

        // Build reverse zone delegation data
        let reverse_zone_delegations: Vec<Value> = {
            use crate::db::models::ReverseDelegationRow;
            use crate::db::schema::nameservers as ns_table;
            use crate::db::schema::{
                reverse_zone_delegation_nameservers, reverse_zone_delegations,
            };

            // Bulk-load all reverse delegation-nameserver pairs in one query
            let rev_del_ns_pairs = reverse_zone_delegation_nameservers::table
                .inner_join(ns_table::table)
                .select((
                    reverse_zone_delegation_nameservers::delegation_id,
                    ns_table::name,
                ))
                .order(ns_table::name.asc())
                .load::<(Uuid, String)>(connection)?;
            let mut rev_del_ns_map: HashMap<Uuid, Vec<String>> = HashMap::new();
            for (delegation_id, name) in rev_del_ns_pairs {
                rev_del_ns_map.entry(delegation_id).or_default().push(name);
            }

            let rows = reverse_zone_delegations::table
                .select(ReverseDelegationRow::as_select())
                .order(reverse_zone_delegations::name.asc())
                .load::<ReverseDelegationRow>(connection)?;
            rows.into_iter()
                .map(|row| {
                    let ns = rev_del_ns_map.remove(&row.id()).unwrap_or_default();
                    let ns_names: Vec<crate::domain::types::DnsName> = ns
                        .iter()
                        .map(crate::domain::types::DnsName::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    let delegation = row.into_reverse_delegation(ns_names)?;
                    Ok(json!({
                        "name": delegation.name().as_str(),
                        "zone_id": delegation.zone_id().to_string(),
                        "comment": delegation.comment(),
                        "nameservers": ns,
                    }))
                })
                .collect::<Result<Vec<_>, AppError>>()?
        };

        Ok(json!({
            "scope": run.scope(),
            "parameters": run.parameters(),
            "labels": labels.iter().map(|label| json!({
                "name": label.name().as_str(),
                "description": label.description(),
            })).collect::<Vec<_>>(),
            "nameservers": nameservers.iter().map(|ns| json!({
                "name": ns.name().as_str(),
                "ttl": ns.ttl().map(|ttl| ttl.as_u32()),
            })).collect::<Vec<_>>(),
            "forward_zones": forward_zones.iter().map(|zone| json!({
                "name": zone.name().as_str(),
                "primary_ns": zone.primary_ns().as_str(),
                "email": zone.email().as_str(),
                "serial_no": zone.serial_no().as_u64(),
                "refresh": zone.refresh(),
                "retry": zone.retry(),
                "expire": zone.expire(),
                "soa_ttl": zone.soa_ttl().as_u32(),
                "default_ttl": zone.default_ttl().as_u32(),
                "nameservers": zone.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "forward_zone_delegations": forward_zone_delegations,
            "reverse_zones": reverse_zones.iter().map(|zone| json!({
                "name": zone.name().as_str(),
                "network": zone.network().map(|value| value.as_str()),
                "primary_ns": zone.primary_ns().as_str(),
                "email": zone.email().as_str(),
                "serial_no": zone.serial_no().as_u64(),
                "refresh": zone.refresh(),
                "retry": zone.retry(),
                "expire": zone.expire(),
                "soa_ttl": zone.soa_ttl().as_u32(),
                "default_ttl": zone.default_ttl().as_u32(),
                "nameservers": zone.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "reverse_zone_delegations": reverse_zone_delegations,
            "networks": networks.iter().map(|network| json!({
                "cidr": network.cidr().as_str(),
                "description": network.description(),
                "reserved": network.reserved(),
            })).collect::<Vec<_>>(),
            "hosts": hosts.iter().map(|host| json!({
                "id": host.id().to_string(),
                "name": host.name().as_str(),
                "zone": host.zone().map(|zone| zone.as_str()),
                "ttl": host.ttl().map(|ttl| ttl.as_u32()),
                "comment": host.comment(),
            })).collect::<Vec<_>>(),
            "ip_addresses": ip_addresses.iter().map(|assignment| json!({
                "host_id": assignment.host_id().to_string(),
                "address": assignment.address().as_str(),
                "family": assignment.family(),
                "mac_address": assignment.mac_address().map(|m| m.as_str()),
            })).collect::<Vec<_>>(),
            "record_types": record_types.iter().map(|record_type| json!({
                "name": record_type.name().as_str(),
                "built_in": record_type.built_in(),
            })).collect::<Vec<_>>(),
            "rrsets": rrsets.iter().map(|rrset| json!({
                "type_name": rrset.type_name().as_str(),
                "owner_name": rrset.owner_name().as_str(),
                "ttl": rrset.ttl().map(|ttl| ttl.as_u32()),
            })).collect::<Vec<_>>(),
            "records": records.iter().map(|record| {
                let dns_type = record_types.iter()
                    .find(|rt| rt.name() == record.type_name())
                    .and_then(|rt| rt.dns_type());
                let rfc3597_rendered = record.raw_rdata().map(|raw| {
                    if let Some(type_num) = dns_type {
                        format!("TYPE{} {}", type_num, raw.presentation())
                    } else {
                        format!("{} {}", record.type_name().as_str(), raw.presentation())
                    }
                });
                json!({
                    "type_name": record.type_name().as_str(),
                    "dns_type": dns_type,
                    "owner_name": record.owner_name(),
                    "data": record.data(),
                    "raw_rdata": record.raw_rdata().map(|raw| raw.presentation()),
                    "rendered": record.rendered().map(str::to_string).or(rfc3597_rendered),
                })
            }).collect::<Vec<_>>(),
        }))
    }

    fn dhcp_export_context(
        connection: &mut PgConnection,
        run: &ExportRun,
    ) -> Result<(Value, Vec<String>), AppError> {
        let networks = Self::query_networks(connection)?;
        let attachments = Self::query_attachments(connection)?;
        let ip_addresses = Self::query_ip_addresses(connection)?;
        let all_dhcp_identifiers = Self::query_all_dhcp_identifiers(connection)?;
        let all_prefix_reservations = Self::query_all_prefix_reservations(connection)?;
        let assignment_rows = sql_query(
            "SELECT aca.id,
                    aca.attachment_id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    c.id AS community_id,
                    c.name::text AS community_name,
                    np.name::text AS policy_name,
                    aca.created_at,
                    aca.updated_at
             FROM attachment_community_assignments aca
             JOIN host_attachments a ON a.id = aca.attachment_id
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             JOIN communities c ON c.id = aca.community_id
             JOIN network_policies np ON np.id = c.policy_id
             ORDER BY h.name, n.network, c.name",
        )
        .load::<AttachmentCommunityAssignmentRow>(connection)?;
        let assignments = assignment_rows
            .into_iter()
            .map(AttachmentCommunityAssignmentRow::into_domain)
            .collect::<Result<Vec<_>, _>>()?;

        let mut warnings = Vec::new();
        let mut networks_sorted = networks;
        networks_sorted.sort_by_key(|network| network.cidr().as_str());

        let networks = networks_sorted
            .into_iter()
            .map(|network| {
                let mut network_attachments: Vec<_> = attachments
                    .iter()
                    .filter(|attachment| attachment.network_id() == network.id())
                    .cloned()
                    .collect();
                network_attachments.sort_by_key(|attachment| {
                    (
                        attachment.host_name().as_str().to_string(),
                        attachment.mac_address().map(|value| value.as_str()).unwrap_or_default(),
                    )
                });

                let attachment_json = network_attachments
                    .into_iter()
                    .map(|attachment| {
                        let mut identifiers: Vec<_> = all_dhcp_identifiers
                            .iter()
                            .filter(|identifier| identifier.attachment_id() == attachment.id())
                            .cloned()
                            .collect();
                        identifiers.sort_by_key(|identifier| {
                            (
                                identifier.family().as_u8(),
                                identifier.priority(),
                                dhcp_identifier_kind_name(identifier.kind()).to_string(),
                                identifier.value().to_string(),
                            )
                        });
                        let prefixes: Vec<_> = all_prefix_reservations
                            .iter()
                            .filter(|reservation| reservation.attachment_id() == attachment.id())
                            .cloned()
                            .collect();
                        let mut attachment_ips: Vec<_> = ip_addresses
                            .iter()
                            .filter(|assignment| assignment.attachment_id() == attachment.id())
                            .cloned()
                            .collect();
                        attachment_ips.sort_by_key(|assignment| assignment.address().as_str());
                        let mut attachment_assignments: Vec<_> = assignments
                            .iter()
                            .filter(|assignment| assignment.attachment_id() == attachment.id())
                            .cloned()
                            .collect();
                        attachment_assignments.sort_by_key(|assignment| {
                            (
                                assignment.policy_name().as_str().to_string(),
                                assignment.community_name().as_str().to_string(),
                            )
                        });

                        let ipv4_matcher = identifiers
                            .iter()
                            .find(|identifier| identifier.family().as_u8() == 4)
                            .map(|identifier| {
                                json!({
                                    "kind": dhcp_identifier_kind_name(identifier.kind()),
                                    "value": identifier.value(),
                                })
                            })
                            .or_else(|| {
                                attachment.mac_address().map(|mac| {
                                    json!({"kind": "mac_address", "value": mac.as_str()})
                                })
                            });
                        let ipv6_matcher = identifiers
                            .iter()
                            .find(|identifier| identifier.family().as_u8() == 6)
                            .map(|identifier| {
                                json!({
                                    "kind": dhcp_identifier_kind_name(identifier.kind()),
                                    "value": identifier.value(),
                                })
                            });
                        let ipv6_count = attachment_ips
                            .iter()
                            .filter(|assignment| assignment.family() == 6)
                            .count();
                        if (ipv6_count > 0 || !prefixes.is_empty()) && ipv6_matcher.is_none() {
                            warnings.push(format!(
                                "attachment '{}' on '{}' has IPv6 reservations but no DHCPv6 identifier",
                                attachment.host_name().as_str(),
                                attachment.network_cidr().as_str()
                            ));
                        }

                        Ok::<_, AppError>(json!({
                            "id": attachment.id().to_string(),
                            "host_id": attachment.host_id().to_string(),
                            "host_name": attachment.host_name().as_str(),
                            "mac_address": attachment.mac_address().map(|value| value.as_str()),
                            "comment": attachment.comment(),
                            "dhcp_identifiers": identifiers.into_iter().map(|identifier| json!({
                                "id": identifier.id().to_string(),
                                "family": identifier.family().as_u8(),
                                "kind": dhcp_identifier_kind_name(identifier.kind()),
                                "value": identifier.value(),
                                "priority": identifier.priority(),
                            })).collect::<Vec<_>>(),
                            "matchers": {
                                "ipv4": ipv4_matcher,
                                "ipv6": ipv6_matcher,
                            },
                            "ip_addresses": attachment_ips.iter().map(|assignment| json!({
                                "id": assignment.id().to_string(),
                                "address": assignment.address().as_str(),
                                "family": assignment.family(),
                            })).collect::<Vec<_>>(),
                            "ipv4_addresses": attachment_ips.iter().filter(|assignment| assignment.family() == 4).map(|assignment| json!({
                                "id": assignment.id().to_string(),
                                "address": assignment.address().as_str(),
                            })).collect::<Vec<_>>(),
                            "ipv6_addresses": attachment_ips.iter().filter(|assignment| assignment.family() == 6).map(|assignment| json!({
                                "id": assignment.id().to_string(),
                                "address": assignment.address().as_str(),
                            })).collect::<Vec<_>>(),
                            "primary_ipv4_address": attachment_ips.iter().find(|assignment| assignment.family() == 4).map(|assignment| assignment.address().as_str()),
                            "primary_ipv6_address": attachment_ips.iter().find(|assignment| assignment.family() == 6).map(|assignment| assignment.address().as_str()),
                            "prefix_reservations": prefixes.into_iter().map(|reservation| json!({
                                "id": reservation.id().to_string(),
                                "prefix": reservation.prefix().as_str(),
                            })).collect::<Vec<_>>(),
                            "community_assignments": attachment_assignments.iter().map(|assignment| json!({
                                "id": assignment.id().to_string(),
                                "policy_name": assignment.policy_name().as_str(),
                                "community_name": assignment.community_name().as_str(),
                            })).collect::<Vec<_>>(),
                        }))
                    })
                    .collect::<Result<Vec<_>, AppError>>()?;
                let dhcp4_attachments = attachment_json
                    .iter()
                    .filter(|attachment| {
                        attachment["matchers"]["ipv4"].is_object()
                            && attachment["primary_ipv4_address"].is_string()
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                let dhcp6_attachments = attachment_json
                    .iter()
                    .filter(|attachment| {
                        attachment["matchers"]["ipv6"].is_object()
                            && (attachment["primary_ipv6_address"].is_string()
                                || attachment["prefix_reservations"]
                                    .as_array()
                                    .is_some_and(|items| !items.is_empty()))
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                let communities = assignments
                    .iter()
                    .filter(|assignment| assignment.network_id() == network.id())
                    .map(|assignment| {
                        json!({
                            "id": assignment.community_id().to_string(),
                            "policy_name": assignment.policy_name().as_str(),
                            "name": assignment.community_name().as_str(),
                        })
                    })
                    .collect::<Vec<_>>();

                Ok::<_, AppError>(json!({
                    "id": network.id().to_string(),
                    "cidr": network.cidr().as_str(),
                    "family": if network.cidr().is_v6() { 6 } else { 4 },
                    "description": network.description(),
                    "vlan": network.vlan(),
                    "dns_delegated": network.dns_delegated(),
                    "category": network.category(),
                    "location": network.location(),
                    "frozen": network.frozen(),
                    "reserved": network.reserved(),
                    "communities": communities,
                    "attachments": attachment_json,
                    "dhcp4_attachments": dhcp4_attachments,
                    "dhcp6_attachments": dhcp6_attachments,
                }))
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        let dhcp4_networks = networks
            .iter()
            .filter(|network| network["family"] == 4)
            .cloned()
            .collect::<Vec<_>>();
        let dhcp6_networks = networks
            .iter()
            .filter(|network| network["family"] == 6)
            .cloned()
            .collect::<Vec<_>>();

        Ok((
            json!({
                "scope": run.scope(),
                "parameters": run.parameters(),
                "warnings": warnings,
                "networks": networks,
                "dhcp4_networks": dhcp4_networks,
                "dhcp6_networks": dhcp6_networks,
            }),
            warnings,
        ))
    }
}

#[async_trait]
impl ExportStore for PostgresStorage {
    async fn list_export_templates(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ExportTemplate>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_export_templates(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn list_export_runs(&self, page: &PageRequest) -> Result<Page<ExportRun>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_export_runs(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn create_export_template(
        &self,
        command: CreateExportTemplate,
    ) -> Result<ExportTemplate, AppError> {
        let name = command.name().to_string();
        let description = command.description().to_string();
        let engine = command.engine().to_string();
        let scope = command.scope().to_string();
        let body = command.body().to_string();
        let metadata = command.metadata().clone();
        self.database
            .run(move |connection| {
                insert_into(export_templates::table)
                    .values((
                        export_templates::name.eq(&name),
                        export_templates::description.eq(&description),
                        export_templates::engine.eq(&engine),
                        export_templates::scope.eq(&scope),
                        export_templates::body.eq(&body),
                        export_templates::metadata.eq(&metadata),
                    ))
                    .returning(ExportTemplateRow::as_returning())
                    .get_result(connection)
                    .map_err(map_unique("export template already exists"))?
                    .into_domain()
            })
            .await
    }

    async fn create_export_run(&self, command: CreateExportRun) -> Result<ExportRun, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<ExportRun, AppError, _>(|connection| {
                    let template_id = export_templates::table
                        .filter(export_templates::name.eq(command.template_name()))
                        .select(export_templates::id)
                        .first::<Uuid>(connection)
                        .optional()?
                        .ok_or_else(|| {
                            AppError::not_found(format!(
                                "export template '{}' was not found",
                                command.template_name()
                            ))
                        })?;
                    let run_id = Uuid::new_v4();
                    let task = Self::create_task_row(
                        connection,
                        &CreateTask::new(
                            "export_run",
                            command.requested_by().map(str::to_string),
                            json!({"run_id": run_id}),
                            None,
                            1,
                        )?,
                        None,
                    )?;
                    insert_into(export_runs::table)
                        .values((
                            export_runs::id.eq(run_id),
                            export_runs::task_id.eq(Some(task.id())),
                            export_runs::template_id.eq(Some(template_id)),
                            export_runs::requested_by.eq(command.requested_by()),
                            export_runs::scope.eq(command.scope()),
                            export_runs::parameters.eq(command.parameters().clone()),
                            export_runs::status.eq("queued"),
                        ))
                        .returning(ExportRunRow::as_returning())
                        .get_result(connection)?
                        .into_domain()
                })
            })
            .await
    }

    async fn run_export(&self, run_id: Uuid) -> Result<ExportRun, AppError> {
        let result = self
            .database
            .run(move |connection| {
                connection.transaction::<ExportRun, AppError, _>(|connection| {
                    // NOTE: FOR UPDATE locking requires sql_query
                    let run = sql_query(
                        "SELECT id, task_id, template_id, requested_by, scope, parameters, status,
                                rendered_output, artifact_metadata, created_at, updated_at
                         FROM export_runs
                         WHERE id = $1
                         FOR UPDATE",
                    )
                    .bind::<SqlUuid, _>(run_id)
                    .get_result::<ExportRunRow>(connection)
                    .map_err(|_| {
                        AppError::not_found(format!("export run '{}' was not found", run_id))
                    })?
                    .into_domain()?;

                    let template = export_templates::table
                        .filter(export_templates::id.eq(run.template_id().ok_or_else(|| {
                            AppError::not_found("export run is not linked to a template")
                        })?))
                        .select(ExportTemplateRow::as_select())
                        .first::<ExportTemplateRow>(connection)
                        .optional()?
                        .ok_or_else(|| {
                            AppError::not_found("export template for run was not found")
                        })?
                        .into_domain()?;

                    let (context, warnings) = if run.scope() == "forward_zone"
                        || run.scope() == "reverse_zone"
                    {
                        let zone_name = run
                            .parameters()
                            .get("zone_name")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AppError::validation("zone_name parameter required for zone export")
                            })?;
                        (
                            Self::zone_export_context(connection, &run, zone_name)?,
                            Vec::new(),
                        )
                    } else if run.scope() == "dhcp" {
                        Self::dhcp_export_context(connection, &run)?
                    } else {
                        (Self::export_context(connection, &run)?, Vec::new())
                    };

                    let rendered = render_export_template(&template, &context)?;

                    update(export_runs::table.filter(export_runs::id.eq(run_id)))
                        .set((
                            export_runs::status.eq("succeeded"),
                            export_runs::rendered_output.eq(Some(&rendered)),
                            export_runs::artifact_metadata
                                .eq(Some(json!({"bytes": rendered.len(), "warnings": warnings}))),
                            export_runs::updated_at.eq(diesel::dsl::now),
                        ))
                        .returning(ExportRunRow::as_returning())
                        .get_result::<ExportRunRow>(connection)?
                        .into_domain()
                })
            })
            .await;

        match result {
            Ok(run) => Ok(run),
            Err(error) => {
                let message = error.to_string();
                let _ = self
                    .database
                    .run(move |connection| {
                        sql_query(
                            "UPDATE export_runs
                             SET status = 'failed', artifact_metadata = COALESCE(artifact_metadata, $2), updated_at = now()
                             WHERE id = $1",
                        )
                        .bind::<SqlUuid, _>(run_id)
                        .bind::<diesel::sql_types::Jsonb, _>(json!({"error": message}))
                        .execute(connection)?;
                        Ok(())
                    })
                    .await;
                Err(error)
            }
        }
    }
}
