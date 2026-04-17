use async_trait::async_trait;
use chrono::Utc;
use minijinja::Environment;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    domain::{
        exports::{
            CreateExportRun, CreateExportTemplate, ExportRun, ExportRunStatus, ExportTemplate,
        },
        pagination::{Page, PageRequest},
        tasks::{CreateTask, TaskEnvelope, TaskStatus},
    },
    errors::AppError,
    storage::ExportStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, tasks::create_task_in_state};

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

fn build_dhcp_attachment_export(
    state: &MemoryState,
    attachment: &crate::domain::attachment::HostAttachment,
) -> (Value, Vec<String>) {
    let mut warnings = Vec::new();
    let mut identifiers: Vec<_> = state
        .attachment_dhcp_identifiers
        .values()
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

    let mut ip_addresses: Vec<_> = state
        .ip_addresses
        .values()
        .filter(|assignment| assignment.attachment_id() == attachment.id())
        .cloned()
        .collect();
    ip_addresses.sort_by_key(|assignment| assignment.address().as_str());

    let mut prefixes: Vec<_> = state
        .attachment_prefix_reservations
        .values()
        .filter(|reservation| reservation.attachment_id() == attachment.id())
        .cloned()
        .collect();
    prefixes.sort_by_key(|reservation| reservation.prefix().as_str());

    let mut assignments: Vec<_> = state
        .attachment_community_assignments
        .values()
        .filter(|assignment| assignment.attachment_id() == attachment.id())
        .cloned()
        .collect();
    assignments.sort_by_key(|assignment| {
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
            attachment
                .mac_address()
                .map(|mac| json!({"kind": "mac_address", "value": mac.as_str()}))
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

    let ipv4_addresses: Vec<Value> = ip_addresses
        .iter()
        .filter(|assignment| assignment.family() == 4)
        .map(|assignment| {
            json!({
                "id": assignment.id().to_string(),
                "address": assignment.address().as_str(),
            })
        })
        .collect();
    let ipv6_addresses: Vec<Value> = ip_addresses
        .iter()
        .filter(|assignment| assignment.family() == 6)
        .map(|assignment| {
            json!({
                "id": assignment.id().to_string(),
                "address": assignment.address().as_str(),
            })
        })
        .collect();

    if (!ipv6_addresses.is_empty() || !prefixes.is_empty()) && ipv6_matcher.is_none() {
        warnings.push(format!(
            "attachment '{}' on '{}' has IPv6 reservations but no DHCPv6 identifier",
            attachment.host_name().as_str(),
            attachment.network_cidr().as_str()
        ));
    }

    (
        json!({
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
            "ip_addresses": ip_addresses.iter().map(|assignment| json!({
                "id": assignment.id().to_string(),
                "address": assignment.address().as_str(),
                "family": assignment.family(),
            })).collect::<Vec<_>>(),
            "ipv4_addresses": ipv4_addresses,
            "ipv6_addresses": ipv6_addresses,
            "primary_ipv4_address": ip_addresses.iter().find(|assignment| assignment.family() == 4).map(|assignment| assignment.address().as_str()),
            "primary_ipv6_address": ip_addresses.iter().find(|assignment| assignment.family() == 6).map(|assignment| assignment.address().as_str()),
            "prefix_reservations": prefixes.into_iter().map(|reservation| json!({
                "id": reservation.id().to_string(),
                "prefix": reservation.prefix().as_str(),
            })).collect::<Vec<_>>(),
            "community_assignments": assignments.into_iter().map(|assignment| json!({
                "id": assignment.id().to_string(),
                "policy_name": assignment.policy_name().as_str(),
                "community_name": assignment.community_name().as_str(),
            })).collect::<Vec<_>>(),
        }),
        warnings,
    )
}

fn dhcp_export_context(state: &MemoryState, run: &ExportRun) -> (Value, Vec<String>) {
    let mut warnings = Vec::new();
    let mut networks: Vec<_> = state.networks.values().cloned().collect();
    networks.sort_by_key(|network| network.cidr().as_str());

    let networks = networks
        .into_iter()
        .map(|network| {
            let mut attachments: Vec<_> = state
                .host_attachments
                .values()
                .filter(|attachment| attachment.network_id() == network.id())
                .cloned()
                .collect();
            attachments.sort_by_key(|attachment| {
                (
                    attachment.host_name().as_str().to_string(),
                    attachment
                        .mac_address()
                        .map(|value| value.as_str())
                        .unwrap_or_default(),
                )
            });
            let attachment_json = attachments
                .into_iter()
                .map(|attachment| {
                    let (value, mut attachment_warnings) =
                        build_dhcp_attachment_export(state, &attachment);
                    warnings.append(&mut attachment_warnings);
                    value
                })
                .collect::<Vec<_>>();
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
            let communities = state
                .communities
                .values()
                .filter(|community| community.network_cidr() == network.cidr())
                .map(|community| {
                    json!({
                        "id": community.id().to_string(),
                        "policy_name": community.policy_name().as_str(),
                        "name": community.name().as_str(),
                        "description": community.description(),
                    })
                })
                .collect::<Vec<_>>();
            json!({
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
            })
        })
        .collect::<Vec<_>>();

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

    (
        json!({
            "scope": run.scope(),
            "parameters": run.parameters(),
            "warnings": warnings,
            "networks": networks,
            "dhcp4_networks": dhcp4_networks,
            "dhcp6_networks": dhcp6_networks,
        }),
        warnings,
    )
}

fn render_export_template(template: &ExportTemplate, context: &Value) -> Result<String, AppError> {
    match template.engine() {
        "json" => serde_json::to_string_pretty(context).map_err(AppError::internal),
        "minijinja" => {
            let mut env = Environment::new();
            env.add_template("export", template.body())
                .map_err(AppError::internal)?;
            let template_ref = env.get_template("export").map_err(AppError::internal)?;
            template_ref
                .render(minijinja::value::Value::from_serialize(context))
                .map_err(AppError::internal)
        }
        other => Err(AppError::validation(format!(
            "unsupported export template engine '{other}'"
        ))),
    }
}

/// Build a zone-scoped export context for a forward zone.
fn forward_zone_export_context(
    state: &MemoryState,
    run: &ExportRun,
    zone_name: &str,
) -> Result<Value, AppError> {
    let zone = state.forward_zones.get(zone_name).ok_or_else(|| {
        AppError::not_found(format!("forward zone '{}' was not found", zone_name))
    })?;
    let zone_id = zone.id();
    let zone_suffix = format!(".{}", zone_name);

    let zone_json = json!({
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

    let delegations: Vec<Value> = state
        .forward_zone_delegations
        .values()
        .filter(|d| d.zone_id() == zone_id)
        .map(|d| {
            json!({
                "name": d.name().as_str(),
                "nameservers": d.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
                "comment": d.comment(),
            })
        })
        .collect();

    let records: Vec<Value> = state
        .records
        .iter()
        .filter(|record| record.zone_id() == Some(zone_id))
        .map(|record| {
            let dns_type = state
                .record_types
                .get(record.type_name().as_str())
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

    let hosts: Vec<Value> = state
        .hosts
        .values()
        .filter(|host| {
            host.zone().map(|z| z.as_str()) == Some(zone_name)
                || host.name().as_str() == zone_name
                || host.name().as_str().ends_with(&zone_suffix)
        })
        .map(|host| {
            json!({
                "id": host.id().to_string(),
                "name": host.name().as_str(),
                "ttl": host.ttl().map(|ttl| ttl.as_u32()),
                "comment": host.comment(),
            })
        })
        .collect();

    let host_ids: std::collections::HashSet<String> = hosts
        .iter()
        .filter_map(|h| h.get("id").and_then(|v| v.as_str()).map(str::to_string))
        .collect();

    let ip_addresses: Vec<Value> = state
        .ip_addresses
        .values()
        .filter(|assignment| host_ids.contains(&assignment.host_id().to_string()))
        .map(|assignment| {
            json!({
                "host_id": assignment.host_id().to_string(),
                "address": assignment.address().as_str(),
                "family": assignment.family(),
                "mac_address": assignment.mac_address().map(|m| m.as_str()),
            })
        })
        .collect();

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

/// Build a zone-scoped export context for a reverse zone.
fn reverse_zone_export_context(
    state: &MemoryState,
    run: &ExportRun,
    zone_name: &str,
) -> Result<Value, AppError> {
    let zone = state.reverse_zones.get(zone_name).ok_or_else(|| {
        AppError::not_found(format!("reverse zone '{}' was not found", zone_name))
    })?;
    let zone_id = zone.id();

    let zone_json = json!({
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

    let delegations: Vec<Value> = state
        .reverse_zone_delegations
        .values()
        .filter(|d| d.zone_id() == zone_id)
        .map(|d| {
            json!({
                "name": d.name().as_str(),
                "nameservers": d.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
                "comment": d.comment(),
            })
        })
        .collect();

    let records: Vec<Value> = state
        .records
        .iter()
        .filter(|record| record.zone_id() == Some(zone_id))
        .map(|record| {
            let dns_type = state
                .record_types
                .get(record.type_name().as_str())
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

    Ok(json!({
        "zone": zone_json,
        "delegations": delegations,
        "records": records,
        "hosts": [],
        "ip_addresses": [],
        "scope": run.scope(),
        "parameters": run.parameters(),
    }))
}

fn export_context(state: &MemoryState, run: &ExportRun) -> Value {
    json!({
        "scope": run.scope(),
        "parameters": run.parameters(),
        "labels": state.labels.values().map(|label| json!({
            "name": label.name().as_str(),
            "description": label.description(),
        })).collect::<Vec<_>>(),
        "nameservers": state.nameservers.values().map(|nameserver| json!({
            "name": nameserver.name().as_str(),
            "ttl": nameserver.ttl().map(|value| value.as_u32()),
        })).collect::<Vec<_>>(),
        "forward_zones": state.forward_zones.values().map(|zone| json!({
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
        "forward_zone_delegations": state.forward_zone_delegations.values().map(|d| json!({
            "name": d.name().as_str(),
            "zone_id": d.zone_id().to_string(),
            "comment": d.comment(),
            "nameservers": d.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "reverse_zones": state.reverse_zones.values().map(|zone| json!({
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
        "reverse_zone_delegations": state.reverse_zone_delegations.values().map(|d| json!({
            "name": d.name().as_str(),
            "zone_id": d.zone_id().to_string(),
            "comment": d.comment(),
            "nameservers": d.nameservers().iter().map(|ns| ns.as_str()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "networks": state.networks.values().map(|network| json!({
            "cidr": network.cidr().as_str(),
            "description": network.description(),
            "reserved": network.reserved(),
        })).collect::<Vec<_>>(),
        "hosts": state.hosts.values().map(|host| json!({
            "id": host.id().to_string(),
            "name": host.name().as_str(),
            "zone": host.zone().map(|zone| zone.as_str()),
            "ttl": host.ttl().map(|ttl| ttl.as_u32()),
            "comment": host.comment(),
        })).collect::<Vec<_>>(),
        "ip_addresses": state.ip_addresses.values().map(|assignment| json!({
            "host_id": assignment.host_id().to_string(),
            "address": assignment.address().as_str(),
            "family": assignment.family(),
            "mac_address": assignment.mac_address().map(|m| m.as_str()),
        })).collect::<Vec<_>>(),
        "record_types": state.record_types.values().map(|record_type| json!({
            "name": record_type.name().as_str(),
            "built_in": record_type.built_in(),
        })).collect::<Vec<_>>(),
        "rrsets": state.rrsets.values().map(|rrset| json!({
            "type_name": rrset.type_name().as_str(),
            "owner_name": rrset.owner_name().as_str(),
            "ttl": rrset.ttl().map(|ttl| ttl.as_u32()),
        })).collect::<Vec<_>>(),
        "records": state.records.iter().map(|record| {
            let dns_type = state.record_types.get(record.type_name().as_str())
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
    })
}

#[async_trait]
impl ExportStore for MemoryStorage {
    async fn list_export_templates(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ExportTemplate>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<ExportTemplate> = state.export_templates.values().cloned().collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn list_export_runs(&self, page: &PageRequest) -> Result<Page<ExportRun>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<ExportRun> = state.export_runs.values().cloned().collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn create_export_template(
        &self,
        command: CreateExportTemplate,
    ) -> Result<ExportTemplate, AppError> {
        let mut state = self.state.write().await;
        let key = command.name().to_ascii_lowercase();
        if state.export_templates.contains_key(&key) {
            return Err(AppError::conflict(format!(
                "export template '{}' already exists",
                command.name()
            )));
        }
        let template = ExportTemplate::restore(
            Uuid::new_v4(),
            command.name(),
            command.description(),
            command.engine(),
            command.scope(),
            command.body(),
            command.metadata().clone(),
            false,
        )?;
        state.export_templates.insert(key, template.clone());
        Ok(template)
    }

    async fn create_export_run(&self, command: CreateExportRun) -> Result<ExportRun, AppError> {
        let mut state = self.state.write().await;
        let template = state
            .export_templates
            .get(&command.template_name().to_ascii_lowercase())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "export template '{}' was not found",
                    command.template_name()
                ))
            })?;
        let run_id = Uuid::new_v4();
        let task = create_task_in_state(
            &mut state,
            CreateTask::new(
                "export_run",
                command.requested_by().map(str::to_string),
                json!({ "run_id": run_id }),
                None,
                1,
            )?,
        )?;
        let now = Utc::now();
        let run = ExportRun::restore(
            run_id,
            Some(task.id()),
            Some(template.id()),
            command.requested_by().map(str::to_string),
            command.scope(),
            command.parameters().clone(),
            ExportRunStatus::Queued,
            None,
            None,
            now,
            now,
        )?;
        state.export_runs.insert(run_id, run.clone());
        Ok(run)
    }

    async fn run_export(&self, run_id: Uuid) -> Result<ExportRun, AppError> {
        let mut state = self.state.write().await;
        let run =
            state.export_runs.get(&run_id).cloned().ok_or_else(|| {
                AppError::not_found(format!("export run '{}' was not found", run_id))
            })?;
        let template = state
            .export_templates
            .values()
            .find(|template| Some(template.id()) == run.template_id())
            .cloned()
            .ok_or_else(|| AppError::not_found("export template for run was not found"))?;

        let (context, warnings) = if run.scope() == "forward_zone" || run.scope() == "reverse_zone"
        {
            let zone_name = run
                .parameters()
                .get("zone_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AppError::validation("zone_name parameter required for zone export")
                })?;
            if run.scope() == "forward_zone" {
                (
                    forward_zone_export_context(&state, &run, zone_name)?,
                    Vec::new(),
                )
            } else {
                (
                    reverse_zone_export_context(&state, &run, zone_name)?,
                    Vec::new(),
                )
            }
        } else if run.scope() == "dhcp" {
            dhcp_export_context(&state, &run)
        } else {
            (export_context(&state, &run), Vec::new())
        };
        let rendered = render_export_template(&template, &context)?;
        let now = Utc::now();
        let updated = ExportRun::restore(
            run.id(),
            run.task_id(),
            run.template_id(),
            run.requested_by().map(str::to_string),
            run.scope(),
            run.parameters().clone(),
            ExportRunStatus::Succeeded,
            Some(rendered.clone()),
            Some(json!({"bytes": rendered.len(), "warnings": warnings})),
            run.created_at(),
            now,
        )?;
        state.export_runs.insert(run_id, updated.clone());
        if let Some(task_id) = run.task_id() {
            let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                AppError::internal("export task disappeared from in-memory storage")
            })?;
            let completed = TaskEnvelope::restore(
                task.id(),
                task.kind().to_string(),
                TaskStatus::Succeeded,
                task.payload().clone(),
                json!({"stage":"done"}),
                Some(json!({"run_id": run_id, "bytes": rendered.len()})),
                None,
                task.attempts().max(1),
                task.max_attempts(),
                task.available_at(),
                task.started_at().or(Some(now)),
                Some(now),
            )?;
            state.tasks.insert(task_id, completed);
        }
        Ok(updated)
    }
}
