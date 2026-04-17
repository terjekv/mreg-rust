use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    domain::{
        attachment::{
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, DhcpIdentifierFamily,
            DhcpIdentifierKind,
        },
        bacnet::CreateBacnetIdAssignment,
        community::CreateCommunity,
        host::AssignIpAddress,
        host_community_assignment::CreateHostCommunityAssignment,
        host_contact::CreateHostContact,
        host_group::CreateHostGroup,
        imports::{CreateImportBatch, ImportBatchStatus, ImportBatchSummary, ImportItem},
        label::CreateLabel,
        nameserver::CreateNameServer,
        network::{CreateExcludedRange, CreateNetwork},
        network_policy::CreateNetworkPolicy,
        pagination::{Page, PageRequest},
        ptr_override::CreatePtrOverride,
        resource_records::CreateRecordInstance,
        tasks::{CreateTask, TaskEnvelope, TaskStatus},
        types::{
            BacnetIdentifier, CidrValue, CommunityName, DnsName, EmailAddressValue, HostGroupName,
            Hostname, IpAddressValue, LabelName, MacAddressValue, NetworkPolicyName,
            OwnerGroupName, SerialNumber, Ttl, ZoneName,
        },
        zone::{CreateForwardZone, CreateReverseZone},
    },
    errors::AppError,
    storage::ImportStore,
};

use super::attachments::{
    create_attachment_community_assignment_in_state, create_attachment_dhcp_identifier_in_state,
    create_attachment_in_state, create_attachment_prefix_reservation_in_state,
};
use super::bacnet::create_bacnet_id_in_state;
use super::communities::create_community_in_state;
use super::host_community_assignments::create_host_community_assignment_in_state;
use super::host_contacts::create_host_contact_in_state;
use super::host_groups::create_host_group_in_state;
use super::hosts::assign_ip_in_state;
use super::labels::create_label_in_state;
use super::nameservers::create_nameserver_in_state;
use super::network_policies::create_network_policy_in_state;
use super::networks::{add_excluded_range_in_state, create_network_in_state};
use super::ptr_overrides::create_ptr_override_in_state;
use super::tasks::create_task_in_state;
use super::zones::{create_forward_zone_in_state, create_reverse_zone_in_state};
use super::{
    MemoryState, MemoryStorage, StoredImportBatch, paginate_by_cursor,
    records::create_record_in_state,
};

fn create_host_in_state(
    state: &mut MemoryState,
    command: crate::domain::host::CreateHost,
) -> Result<crate::domain::host::Host, AppError> {
    let key = command.name().as_str().to_string();
    if state.hosts.contains_key(&key) {
        return Err(AppError::conflict(format!("host '{}' already exists", key)));
    }
    if let Some(zone) = command.zone()
        && !state.forward_zones.contains_key(zone.as_str())
    {
        return Err(AppError::not_found(format!(
            "forward zone '{}' was not found",
            zone.as_str()
        )));
    }
    let now = Utc::now();
    let host = crate::domain::host::Host::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.zone().cloned(),
        command.ttl(),
        command.comment().to_string(),
        now,
        now,
    )?;
    state.hosts.insert(key, host.clone());
    Ok(host)
}

fn update_import_status(
    state: &mut MemoryState,
    import_id: Uuid,
    status: ImportBatchStatus,
    validation_report: Option<Value>,
    commit_summary: Option<Value>,
    updated_at: chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let stored = state.imports.get_mut(&import_id).ok_or_else(|| {
        AppError::not_found(format!("import batch '{}' was not found", import_id))
    })?;
    stored.summary = ImportBatchSummary::restore(
        stored.summary.id(),
        stored.summary.task_id(),
        status,
        stored.summary.requested_by().map(str::to_string),
        validation_report,
        commit_summary,
        stored.summary.created_at(),
        updated_at,
    );
    Ok(())
}

fn apply_import_item(
    state: &mut MemoryState,
    item: &ImportItem,
    refs: &mut BTreeMap<String, String>,
) -> Result<Value, AppError> {
    if item.operation() != "create" {
        return Err(AppError::validation(format!(
            "unsupported import operation '{}'",
            item.operation()
        )));
    }
    let attributes = item.attributes();
    let result = match item.kind() {
        "label" => import_label(state, attributes, refs)?,
        "nameserver" => import_nameserver(state, attributes, refs)?,
        "network" => import_network(state, attributes, refs)?,
        "host_contact" => import_host_contact(state, attributes, refs)?,
        "host_group" => import_host_group(state, attributes, refs)?,
        "bacnet_id" => import_bacnet_id(state, attributes, refs)?,
        "ptr_override" => import_ptr_override(state, attributes, refs)?,
        "network_policy" => import_network_policy(state, attributes, refs)?,
        "community" => import_community(state, attributes, refs)?,
        "forward_zone" => import_forward_zone(state, attributes, refs)?,
        "reverse_zone" => import_reverse_zone(state, attributes, refs)?,
        "excluded_range" => import_excluded_range(state, attributes, refs)?,
        "host" => import_host(state, attributes, refs)?,
        "host_attachment" => import_host_attachment(state, attributes, refs)?,
        "ip_address" => import_ip_address(state, attributes, refs)?,
        "record" => import_record(state, attributes, refs)?,
        "attachment_dhcp_identifier" => import_attachment_dhcp_identifier(state, attributes, refs)?,
        "attachment_prefix_reservation" => {
            import_attachment_prefix_reservation(state, attributes, refs)?
        }
        "attachment_community_assignment" => {
            import_attachment_community_assignment(state, attributes, refs)?
        }
        "host_community_assignment" => import_host_community_assignment(state, attributes, refs)?,
        _ => {
            return Err(AppError::validation(format!(
                "unsupported import kind '{}'",
                item.kind()
            )));
        }
    };

    refs.insert(item.reference().to_string(), stringify_ref_value(&result));
    Ok(json!({
        "ref": item.reference(),
        "kind": item.kind(),
        "result": result,
    }))
}

fn import_label(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let label = create_label_in_state(
        state,
        CreateLabel::new(
            LabelName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_string(attributes, "description", refs)?,
        )?,
    )?;
    Ok(Value::String(label.name().as_str().to_string()))
}

fn import_nameserver(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let nameserver = create_nameserver_in_state(
        state,
        CreateNameServer::new(
            DnsName::new(resolve_string(attributes, "name", refs)?)?,
            None,
        ),
    )?;
    Ok(Value::String(nameserver.name().as_str().to_string()))
}

fn import_network(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let network = create_network_in_state(
        state,
        CreateNetwork::new(
            CidrValue::new(resolve_string(attributes, "cidr", refs)?)?,
            resolve_string(attributes, "description", refs)?,
            resolve_u64(attributes, "reserved")?.unwrap_or(3) as u32,
        )?,
    )?;
    Ok(Value::String(network.cidr().as_str()))
}

fn import_host_contact(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let contact = create_host_contact_in_state(
        state,
        CreateHostContact::new(
            EmailAddressValue::new(resolve_string(attributes, "email", refs)?)?,
            resolve_optional_string(attributes, "display_name", refs)?,
            resolve_string_vec(attributes, "hosts", refs)?
                .into_iter()
                .map(Hostname::new)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    )?;
    Ok(Value::String(contact.email().as_str().to_string()))
}

fn import_host_group(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let group = create_host_group_in_state(
        state,
        CreateHostGroup::new(
            HostGroupName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_string(attributes, "description", refs)?,
            resolve_string_vec(attributes, "hosts", refs)?
                .into_iter()
                .map(Hostname::new)
                .collect::<Result<Vec<_>, _>>()?,
            resolve_string_vec(attributes, "parent_groups", refs)?
                .into_iter()
                .map(HostGroupName::new)
                .collect::<Result<Vec<_>, _>>()?,
            resolve_string_vec(attributes, "owner_groups", refs)?
                .into_iter()
                .map(OwnerGroupName::new)
                .collect::<Result<Vec<_>, _>>()?,
        )?,
    )?;
    Ok(Value::String(group.name().as_str().to_string()))
}

fn import_bacnet_id(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let assignment = create_bacnet_id_in_state(
        state,
        CreateBacnetIdAssignment::new(
            BacnetIdentifier::new(resolve_u64(attributes, "bacnet_id")?.ok_or_else(|| {
                AppError::validation("missing required import attribute 'bacnet_id'")
            })? as u32)?,
            Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
        ),
    )?;
    Ok(Value::String(assignment.bacnet_id().as_u32().to_string()))
}

fn import_ptr_override(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let ptr = create_ptr_override_in_state(
        state,
        CreatePtrOverride::new(
            Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
            IpAddressValue::new(resolve_string(attributes, "address", refs)?)?,
            resolve_optional_string(attributes, "target_name", refs)?
                .map(DnsName::new)
                .transpose()?,
        ),
    )?;
    Ok(Value::String(ptr.address().as_str()))
}

fn import_network_policy(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let policy = create_network_policy_in_state(
        state,
        CreateNetworkPolicy::new(
            NetworkPolicyName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_string(attributes, "description", refs)?,
            resolve_optional_string(attributes, "community_template_pattern", refs)?,
        )?,
    )?;
    Ok(Value::String(policy.name().as_str().to_string()))
}

fn import_community(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let community = create_community_in_state(
        state,
        CreateCommunity::new(
            NetworkPolicyName::new(resolve_string(attributes, "policy_name", refs)?)?,
            CidrValue::new(resolve_string(attributes, "network", refs)?)?,
            CommunityName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_string(attributes, "description", refs)?,
        )?,
    )?;
    Ok(Value::String(community.id().to_string()))
}

fn import_forward_zone(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let primary_ns = DnsName::new(resolve_string(attributes, "primary_ns", refs)?)?;
    let zone = create_forward_zone_in_state(
        state,
        CreateForwardZone::new(
            ZoneName::new(resolve_string(attributes, "name", refs)?)?,
            primary_ns.clone(),
            resolve_string_vec(attributes, "nameservers", refs)?
                .into_iter()
                .map(DnsName::new)
                .collect::<Result<Vec<_>, _>>()?,
            EmailAddressValue::new(resolve_string(attributes, "email", refs)?)?,
            SerialNumber::new(resolve_u64(attributes, "serial_no")?.unwrap_or(1))?,
            resolve_u64(attributes, "refresh")?.unwrap_or(10_800) as u32,
            resolve_u64(attributes, "retry")?.unwrap_or(3_600) as u32,
            resolve_u64(attributes, "expire")?.unwrap_or(1_814_400) as u32,
            Ttl::new(resolve_u64(attributes, "soa_ttl")?.unwrap_or(43_200) as u32)?,
            Ttl::new(resolve_u64(attributes, "default_ttl")?.unwrap_or(43_200) as u32)?,
        ),
    )?;
    Ok(Value::String(zone.name().as_str().to_string()))
}

fn import_reverse_zone(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let primary_ns = DnsName::new(resolve_string(attributes, "primary_ns", refs)?)?;
    let zone = create_reverse_zone_in_state(
        state,
        CreateReverseZone::new(
            ZoneName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_optional_string(attributes, "network", refs)?
                .map(CidrValue::new)
                .transpose()?,
            primary_ns.clone(),
            resolve_string_vec(attributes, "nameservers", refs)?
                .into_iter()
                .map(DnsName::new)
                .collect::<Result<Vec<_>, _>>()?,
            EmailAddressValue::new(resolve_string(attributes, "email", refs)?)?,
            SerialNumber::new(resolve_u64(attributes, "serial_no")?.unwrap_or(1))?,
            resolve_u64(attributes, "refresh")?.unwrap_or(10_800) as u32,
            resolve_u64(attributes, "retry")?.unwrap_or(3_600) as u32,
            resolve_u64(attributes, "expire")?.unwrap_or(1_814_400) as u32,
            Ttl::new(resolve_u64(attributes, "soa_ttl")?.unwrap_or(43_200) as u32)?,
            Ttl::new(resolve_u64(attributes, "default_ttl")?.unwrap_or(43_200) as u32)?,
        ),
    )?;
    Ok(Value::String(zone.name().as_str().to_string()))
}

fn import_excluded_range(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let network = CidrValue::new(resolve_string(attributes, "network", refs)?)?;
    let range = add_excluded_range_in_state(
        state,
        &network,
        CreateExcludedRange::new(
            IpAddressValue::new(resolve_string(attributes, "start_ip", refs)?)?,
            IpAddressValue::new(resolve_string(attributes, "end_ip", refs)?)?,
            resolve_string(attributes, "description", refs)?,
        )?,
    )?;
    Ok(Value::String(format!(
        "{}:{}-{}",
        network.as_str(),
        range.start_ip().as_str(),
        range.end_ip().as_str()
    )))
}

fn import_host(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let zone = resolve_optional_string(attributes, "zone", refs)?
        .map(ZoneName::new)
        .transpose()?;
    let host = create_host_in_state(
        state,
        crate::domain::host::CreateHost::new(
            Hostname::new(resolve_string(attributes, "name", refs)?)?,
            zone,
            None,
            resolve_optional_string(attributes, "comment", refs)?.unwrap_or_default(),
        )?,
    )?;
    Ok(Value::String(host.name().as_str().to_string()))
}

fn resolve_attachment_id(
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Option<Uuid>, AppError> {
    resolve_optional_string(attributes, "attachment_id", refs)?
        .map(|raw| {
            Uuid::parse_str(&raw)
                .map_err(|error| AppError::validation(format!("invalid attachment id: {error}")))
        })
        .transpose()
}

fn import_host_attachment(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let attachment = create_attachment_in_state(
        state,
        CreateHostAttachment::new(
            Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
            CidrValue::new(resolve_string(attributes, "network", refs)?)?,
            resolve_optional_string(attributes, "mac_address", refs)?
                .map(MacAddressValue::new)
                .transpose()?,
            resolve_optional_string(attributes, "comment", refs)?,
        ),
    )?;
    Ok(Value::String(attachment.id().to_string()))
}

fn import_ip_address(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let attachment = resolve_attachment_id(attributes, refs)?
        .map(|attachment_id| {
            state
                .host_attachments
                .get(&attachment_id)
                .cloned()
                .ok_or_else(|| AppError::not_found("host attachment was not found"))
        })
        .transpose()?;
    let address = resolve_optional_string(attributes, "address", refs)?
        .map(IpAddressValue::new)
        .transpose()?;
    let network = resolve_optional_string(attributes, "network", refs)?
        .map(CidrValue::new)
        .transpose()?;
    if let Some(attachment) = &attachment {
        if let Some(explicit_network) = &network
            && explicit_network != attachment.network_cidr()
        {
            return Err(AppError::validation(
                "import ip_address network does not match referenced attachment",
            ));
        }
        if let Some(explicit_host) = resolve_optional_string(attributes, "host_name", refs)?
            && explicit_host != attachment.host_name().as_str()
        {
            return Err(AppError::validation(
                "import ip_address host_name does not match referenced attachment",
            ));
        }
    }
    let assignment = assign_ip_in_state(
        state,
        AssignIpAddress::new(
            attachment
                .as_ref()
                .map(|value| value.host_name().clone())
                .unwrap_or(Hostname::new(resolve_string(
                    attributes,
                    "host_name",
                    refs,
                )?)?),
            address,
            network.or_else(|| {
                attachment
                    .as_ref()
                    .map(|value| value.network_cidr().clone())
            }),
            attachment.and_then(|value| value.mac_address().cloned()),
        )?,
    )?;
    Ok(Value::String(assignment.address().as_str()))
}

fn import_attachment_dhcp_identifier(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let attachment_id = resolve_attachment_id(attributes, refs)?.ok_or_else(|| {
        AppError::validation(
            "missing required import attribute 'attachment_id' or 'attachment_id_ref'",
        )
    })?;
    let family = match resolve_u64(attributes, "family")? {
        Some(4) => DhcpIdentifierFamily::V4,
        Some(6) => DhcpIdentifierFamily::V6,
        Some(_) => {
            return Err(AppError::validation(
                "attachment DHCP identifier family must be 4 or 6",
            ));
        }
        None => {
            return Err(AppError::validation(
                "missing required import attribute 'family'",
            ));
        }
    };
    let kind = match resolve_string(attributes, "kind", refs)?.as_str() {
        "client_id" => DhcpIdentifierKind::ClientId,
        "duid_llt" => DhcpIdentifierKind::DuidLlt,
        "duid_en" => DhcpIdentifierKind::DuidEn,
        "duid_ll" => DhcpIdentifierKind::DuidLl,
        "duid_uuid" => DhcpIdentifierKind::DuidUuid,
        "duid_raw" => DhcpIdentifierKind::DuidRaw,
        _ => {
            return Err(AppError::validation(
                "unsupported attachment DHCP identifier kind",
            ));
        }
    };
    let identifier = create_attachment_dhcp_identifier_in_state(
        state,
        CreateAttachmentDhcpIdentifier::new(
            attachment_id,
            family,
            kind,
            resolve_string(attributes, "value", refs)?,
            resolve_u64(attributes, "priority")?.unwrap_or(100) as i32,
        )?,
    )?;
    Ok(Value::String(identifier.id().to_string()))
}

fn import_attachment_prefix_reservation(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let attachment_id = resolve_attachment_id(attributes, refs)?.ok_or_else(|| {
        AppError::validation(
            "missing required import attribute 'attachment_id' or 'attachment_id_ref'",
        )
    })?;
    let reservation = create_attachment_prefix_reservation_in_state(
        state,
        CreateAttachmentPrefixReservation::new(
            attachment_id,
            CidrValue::new(resolve_string(attributes, "prefix", refs)?)?,
        )?,
    )?;
    Ok(Value::String(reservation.id().to_string()))
}

fn import_attachment_community_assignment(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let attachment_id = resolve_attachment_id(attributes, refs)?.ok_or_else(|| {
        AppError::validation(
            "missing required import attribute 'attachment_id' or 'attachment_id_ref'",
        )
    })?;
    let assignment = create_attachment_community_assignment_in_state(
        state,
        CreateAttachmentCommunityAssignment::new(
            attachment_id,
            NetworkPolicyName::new(resolve_string(attributes, "policy_name", refs)?)?,
            CommunityName::new(resolve_string(attributes, "community_name", refs)?)?,
        ),
    )?;
    Ok(Value::String(assignment.id().to_string()))
}

fn import_record(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let record = create_record_in_state(
        state,
        CreateRecordInstance::with_reference(
            crate::domain::types::RecordTypeName::new(resolve_string(
                attributes,
                "type_name",
                refs,
            )?)?,
            resolve_optional_string(attributes, "owner_kind", refs)?
                .map(|raw| serde_json::from_value(Value::String(raw)).map_err(AppError::internal))
                .transpose()?,
            resolve_string(attributes, "owner_name", refs)?,
            resolve_optional_string(attributes, "anchor_name", refs)?,
            resolve_u64(attributes, "ttl")?
                .map(|value| Ttl::new(value as u32))
                .transpose()?,
            attributes.get("data").cloned(),
            resolve_optional_string(attributes, "raw_rdata", refs)?
                .map(crate::domain::resource_records::RawRdataValue::from_presentation)
                .transpose()?,
        )?,
    )?;
    Ok(Value::String(record.id().to_string()))
}

fn import_host_community_assignment(
    state: &mut MemoryState,
    attributes: &Value,
    refs: &BTreeMap<String, String>,
) -> Result<Value, AppError> {
    let mapping = create_host_community_assignment_in_state(
        state,
        CreateHostCommunityAssignment::new(
            Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
            IpAddressValue::new(resolve_string(attributes, "address", refs)?)?,
            NetworkPolicyName::new(resolve_string(attributes, "policy_name", refs)?)?,
            CommunityName::new(resolve_string(attributes, "community_name", refs)?)?,
        ),
    )?;
    Ok(Value::String(mapping.id().to_string()))
}

fn resolve_string(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    resolve_optional_string(attributes, key, refs)?
        .ok_or_else(|| AppError::validation(format!("missing required import attribute '{}'", key)))
}

fn resolve_optional_string(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<Option<String>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;
    if let Some(value) = object.get(key) {
        return value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| {
                AppError::validation(format!("import attribute '{}' must be a string", key))
            });
    }
    let ref_key = format!("{}_ref", key);
    if let Some(value) = object.get(&ref_key) {
        let reference = value.as_str().ok_or_else(|| {
            AppError::validation(format!("import attribute '{}' must be a string", ref_key))
        })?;
        return refs
            .get(reference)
            .cloned()
            .map(Some)
            .ok_or_else(|| AppError::validation(format!("unknown import ref '{}'", reference)));
    }
    Ok(None)
}

fn resolve_string_vec(
    attributes: &Value,
    key: &str,
    refs: &BTreeMap<String, String>,
) -> Result<Vec<String>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;

    match object.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str().map(str::to_string).ok_or_else(|| {
                    AppError::validation(format!(
                        "import attribute '{}' must be an array of strings",
                        key
                    ))
                })
            })
            .collect(),
        Some(_) => Err(AppError::validation(format!(
            "import attribute '{}' must be an array of strings",
            key
        ))),
        None => Ok(Vec::new()),
    }
    .map(|values: Vec<String>| {
        values
            .into_iter()
            .map(|value| refs.get(&value).cloned().unwrap_or(value))
            .collect()
    })
}

fn resolve_u64(attributes: &Value, key: &str) -> Result<Option<u64>, AppError> {
    let object = attributes
        .as_object()
        .ok_or_else(|| AppError::validation("import item attributes must be a JSON object"))?;
    match object.get(key) {
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            AppError::validation(format!("import attribute '{}' must be an integer", key))
        }),
        None => Ok(None),
    }
}

fn stringify_ref_value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

#[async_trait]
impl ImportStore for MemoryStorage {
    async fn list_import_batches(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ImportBatchSummary>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<ImportBatchSummary> = state
            .imports
            .values()
            .map(|stored| stored.summary.clone())
            .collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn create_import_batch(
        &self,
        command: CreateImportBatch,
    ) -> Result<ImportBatchSummary, AppError> {
        let mut state = self.state.write().await;
        let import_id = Uuid::new_v4();
        let task = create_task_in_state(
            &mut state,
            CreateTask::new(
                "import_batch",
                command.requested_by().map(str::to_string),
                json!({ "import_id": import_id }),
                None,
                1,
            )?,
        )?;
        let now = Utc::now();
        let summary = ImportBatchSummary::restore(
            import_id,
            Some(task.id()),
            ImportBatchStatus::Queued,
            command.requested_by().map(str::to_string),
            None,
            None,
            now,
            now,
        );
        state.imports.insert(
            import_id,
            StoredImportBatch {
                batch: command.batch().clone(),
                summary: summary.clone(),
            },
        );
        Ok(summary)
    }

    async fn run_import_batch(&self, import_id: Uuid) -> Result<ImportBatchSummary, AppError> {
        let mut state = self.state.write().await;
        let stored = state.imports.get(&import_id).cloned().ok_or_else(|| {
            AppError::not_found(format!("import batch '{}' was not found", import_id))
        })?;
        let mut candidate = state.clone();
        let now = Utc::now();

        update_import_status(
            &mut candidate,
            import_id,
            ImportBatchStatus::Validating,
            None,
            None,
            now,
        )?;

        let run_result = (|| -> Result<(Value, Option<Uuid>), AppError> {
            let mut applied = Vec::new();
            let mut refs = BTreeMap::new();
            for item in stored.batch.items() {
                let applied_ref = apply_import_item(&mut candidate, item, &mut refs)?;
                applied.push(applied_ref);
            }

            let commit_summary = json!({
                "applied": applied,
                "count": stored.batch.items().len(),
            });
            Ok((commit_summary, stored.summary.task_id()))
        })();

        match run_result {
            Ok((commit_summary, task_id)) => {
                update_import_status(
                    &mut candidate,
                    import_id,
                    ImportBatchStatus::Succeeded,
                    Some(json!({"valid": true})),
                    Some(commit_summary.clone()),
                    now,
                )?;

                if let Some(task_id) = task_id {
                    let task = candidate.tasks.get(&task_id).cloned().ok_or_else(|| {
                        AppError::internal("import task disappeared from in-memory storage")
                    })?;
                    let completed = TaskEnvelope::restore(
                        task.id(),
                        task.kind().to_string(),
                        TaskStatus::Succeeded,
                        task.payload().clone(),
                        json!({"stage":"done"}),
                        Some(commit_summary),
                        None,
                        task.attempts().max(1),
                        task.max_attempts(),
                        task.available_at(),
                        task.started_at().or(Some(now)),
                        Some(now),
                    )?;
                    candidate.tasks.insert(task_id, completed);
                }

                *state = candidate;
                Ok(state
                    .imports
                    .get(&import_id)
                    .expect("import batch present after update")
                    .summary
                    .clone())
            }
            Err(error) => {
                let message = error.to_string();
                update_import_status(
                    &mut state,
                    import_id,
                    ImportBatchStatus::Failed,
                    Some(json!({"error": message.clone()})),
                    None,
                    now,
                )?;

                if let Some(task_id) = stored.summary.task_id() {
                    let task = state.tasks.get(&task_id).cloned().ok_or_else(|| {
                        AppError::internal("import task disappeared from in-memory storage")
                    })?;
                    let failed = TaskEnvelope::restore(
                        task.id(),
                        task.kind().to_string(),
                        TaskStatus::Failed,
                        task.payload().clone(),
                        task.progress().clone(),
                        None,
                        Some(message),
                        task.attempts().max(1),
                        task.max_attempts(),
                        task.available_at(),
                        task.started_at().or(Some(now)),
                        Some(now),
                    )?;
                    state.tasks.insert(task_id, failed);
                }

                Err(error)
            }
        }
    }
}
