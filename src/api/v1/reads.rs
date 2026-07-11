//! Direct legacy read adapters for data already present in mreg-rust.

use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use actix_web::{HttpRequest, HttpResponse, web};
use ipnet::IpNet;
use rand::seq::SliceRandom;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AppState,
    api::v2::authz::{request as authz_request, require},
    authz::actions,
    domain::{
        filters::{HostContactFilter, HostFilter, NetworkFilter, PtrOverrideFilter, RecordFilter},
        host::IpAddressAssignment,
        host_group::{CreateHostGroup, HostGroup},
        pagination::PageRequest,
        types::{
            CidrValue, DnsName, HostGroupName, HostPolicyName, Hostname, IpAddressValue,
            OwnerGroupName, ZoneName,
        },
        zone::{
            CreateForwardZoneDelegation, ForwardZone, ForwardZoneDelegation, ReverseZoneDelegation,
        },
    },
    errors::AppError,
};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/hosts/{name}/contacts/", web::get().to(host_contacts))
        .service(
            web::resource("/hostgroups/{name}/groups/")
                .route(web::get().to(host_group_groups))
                .route(web::post().to(host_group_group_add)),
        )
        .service(
            web::resource("/hostgroups/{name}/groups/{member}")
                .route(web::delete().to(host_group_group_remove)),
        )
        .service(
            web::resource("/hostgroups/{name}/hosts/")
                .route(web::get().to(host_group_hosts))
                .route(web::post().to(host_group_host_add)),
        )
        .service(
            web::resource("/hostgroups/{name}/hosts/{member}")
                .route(web::delete().to(host_group_host_remove)),
        )
        .service(
            web::resource("/hostgroups/{name}/owners/")
                .route(web::get().to(host_group_owners))
                .route(web::post().to(host_group_owner_add)),
        )
        .service(
            web::resource("/hostgroups/{name}/owners/{member}")
                .route(web::delete().to(host_group_owner_remove)),
        )
        .route("/networks/ip/{ip}", web::get().to(network_by_ip))
        .route(
            "/networks/{network:.*}/first_unused",
            web::get().to(network_first_unused),
        )
        .route(
            "/networks/{network:.*}/random_unused",
            web::get().to(network_random_unused),
        )
        .route(
            "/networks/{network:.*}/ptroverride_list",
            web::get().to(network_ptr_list),
        )
        .route(
            "/networks/{network:.*}/ptroverride_host_list",
            web::get().to(network_ptr_host_list),
        )
        .route(
            "/networks/{network:.*}/reserved_list",
            web::get().to(network_reserved_list),
        )
        .route(
            "/networks/{network:.*}/used_count",
            web::get().to(network_used_count),
        )
        .route(
            "/networks/{network:.*}/used_list",
            web::get().to(network_used_list),
        )
        .route(
            "/networks/{network:.*}/used_host_list",
            web::get().to(network_used_host_list),
        )
        .route(
            "/networks/{network:.*}/unused_count",
            web::get().to(network_unused_count),
        )
        .route(
            "/networks/{network:.*}/unused_list",
            web::get().to(network_unused_list),
        )
        .route("/dhcphosts/ipv4/", web::get().to(dhcp_hosts_v4))
        .route("/dhcphosts/ipv6/", web::get().to(dhcp_hosts_v6))
        .route("/dhcphosts/ipv6byipv4/", web::get().to(dhcp_v6_by_v4_all))
        .route(
            "/dhcphosts/ipv6byipv4/{ip}/{prefix}",
            web::get().to(dhcp_v6_by_v4_range),
        )
        .route("/dhcphosts/{ip}/{prefix}", web::get().to(dhcp_hosts_range))
        .route(
            "/zones/forward/hostname/{hostname}",
            web::get().to(forward_zone_by_hostname),
        )
        .route(
            "/zones/forward/{name:.*}/nameservers",
            web::get().to(forward_zone_nameservers),
        )
        .service(
            web::resource("/zones/forward/{name:.*}/delegations/")
                .route(web::get().to(forward_delegations))
                .route(web::post().to(create_forward_delegation)),
        )
        .route(
            "/zones/reverse/{name:.*}/nameservers",
            web::get().to(reverse_zone_nameservers),
        )
        .service(
            web::resource("/zones/forward/{name:.*}/delegations/{delegation:.*}")
                .route(web::get().to(forward_delegation_detail))
                .route(web::patch().to(update_forward_delegation))
                .route(web::delete().to(delete_forward_delegation)),
        )
        .route(
            "/zones/reverse/{name:.*}/delegations/{delegation:.*}",
            web::get().to(reverse_delegation_detail),
        )
        .service(
            web::resource("/hostpolicy/roles/{name}/atoms/")
                .route(web::get().to(host_policy_role_atoms))
                .route(web::post().to(host_policy_role_atom_add)),
        )
        .service(
            web::resource("/hostpolicy/roles/{name}/atoms/{member}")
                .route(web::delete().to(host_policy_role_atom_remove)),
        )
        .service(
            web::resource("/hostpolicy/roles/{name}/hosts/")
                .route(web::get().to(host_policy_role_hosts))
                .route(web::post().to(host_policy_role_host_add)),
        )
        .service(
            web::resource("/hostpolicy/roles/{name}/hosts/{member}")
                .route(web::delete().to(host_policy_role_host_remove)),
        )
        .route("/zonefiles/{name:.*}", web::get().to(zone_file));
}

pub(super) async fn authorize(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    action: &str,
    kind: &str,
    id: &str,
) -> Result<(), AppError> {
    require(state, authz_request(req, action, kind, id)).await
}

pub(super) fn legacy_page(results: Vec<Value>) -> Value {
    json!({"count": results.len(), "next": null, "previous": null, "results": results})
}

async fn host_contacts(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = Hostname::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_contact::LIST,
        actions::resource_kinds::HOST_CONTACT,
        name.as_str(),
    )
    .await?;
    state.services.hosts().get(&name).await?;
    let filter = HostContactFilter::from_query_params(HashMap::from([(
        "host".to_string(),
        name.as_str().to_string(),
    )]))?;
    let page = state
        .services
        .host_contacts()
        .list(&PageRequest::all(), &filter)
        .await?;
    let values = page
        .items
        .iter()
        .map(|contact| {
            json!({
                "id": contact.id(), "email": contact.email().as_str(),
                "created_at": contact.created_at(), "updated_at": contact.updated_at(),
            })
        })
        .collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(values))
}

async fn get_host_group(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    name: String,
) -> Result<HostGroup, AppError> {
    let name = HostGroupName::new(name)?;
    authorize(
        req,
        state,
        actions::host_group::GET,
        actions::resource_kinds::HOST_GROUP,
        name.as_str(),
    )
    .await?;
    state.services.host_groups().get(&name).await
}

async fn host_group_groups(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let group = get_host_group(&req, &state, name.into_inner()).await?;
    let groups = state
        .services
        .host_groups()
        .list(&PageRequest::all(), &Default::default())
        .await?
        .items;
    Ok(HttpResponse::Ok().json(legacy_page(
        groups
            .iter()
            .filter(|value| {
                value
                    .parent_groups()
                    .iter()
                    .any(|parent| parent == group.name())
            })
            .map(|value| json!({"name": value.name().as_str()}))
            .collect(),
    )))
}

async fn host_group_hosts(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let group = get_host_group(&req, &state, name.into_inner()).await?;
    Ok(HttpResponse::Ok().json(legacy_page(
        group
            .hosts()
            .iter()
            .map(|value| json!({"name": value.as_str()}))
            .collect(),
    )))
}

async fn host_group_owners(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let group = get_host_group(&req, &state, name.into_inner()).await?;
    Ok(HttpResponse::Ok().json(legacy_page(
        group
            .owner_groups()
            .iter()
            .map(|value| json!({"name": value.as_str()}))
            .collect(),
    )))
}

#[derive(Deserialize)]
struct LegacyNamedMember {
    name: String,
}

async fn replace_host_group(
    state: &AppState,
    group: HostGroup,
    hosts: Vec<Hostname>,
    groups: Vec<HostGroupName>,
    owners: Vec<OwnerGroupName>,
) -> Result<(), AppError> {
    state.services.host_groups().delete(group.name()).await?;
    state
        .services
        .host_groups()
        .create(CreateHostGroup::new(
            group.name().clone(),
            group.description(),
            hosts,
            groups,
            owners,
        )?)
        .await?;
    Ok(())
}

async fn host_group_host_add(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyNamedMember>,
) -> Result<HttpResponse, AppError> {
    let group = get_host_group(&req, &state, name.into_inner()).await?;
    let mut hosts = group.hosts().to_vec();
    hosts.push(Hostname::new(payload.into_inner().name)?);
    replace_host_group(
        &state,
        group.clone(),
        hosts,
        group.parent_groups().to_vec(),
        group.owner_groups().to_vec(),
    )
    .await?;
    Ok(HttpResponse::Created().finish())
}

async fn host_group_host_remove(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (name, member) = path.into_inner();
    let group = get_host_group(&req, &state, name).await?;
    let member = Hostname::new(member)?;
    let hosts = group
        .hosts()
        .iter()
        .filter(|value| *value != &member)
        .cloned()
        .collect();
    replace_host_group(
        &state,
        group.clone(),
        hosts,
        group.parent_groups().to_vec(),
        group.owner_groups().to_vec(),
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn host_group_group_add(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyNamedMember>,
) -> Result<HttpResponse, AppError> {
    let parent = get_host_group(&req, &state, name.into_inner()).await?;
    let child = get_host_group(&req, &state, payload.into_inner().name).await?;
    let mut groups = child.parent_groups().to_vec();
    groups.push(parent.name().clone());
    replace_host_group(
        &state,
        child.clone(),
        child.hosts().to_vec(),
        groups,
        child.owner_groups().to_vec(),
    )
    .await?;
    Ok(HttpResponse::Created().finish())
}

async fn host_group_group_remove(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (name, member) = path.into_inner();
    let parent = get_host_group(&req, &state, name).await?;
    let child = get_host_group(&req, &state, member).await?;
    let groups = child
        .parent_groups()
        .iter()
        .filter(|value| *value != parent.name())
        .cloned()
        .collect();
    replace_host_group(
        &state,
        child.clone(),
        child.hosts().to_vec(),
        groups,
        child.owner_groups().to_vec(),
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn host_group_owner_add(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyNamedMember>,
) -> Result<HttpResponse, AppError> {
    let group = get_host_group(&req, &state, name.into_inner()).await?;
    let mut owners = group.owner_groups().to_vec();
    owners.push(OwnerGroupName::new(payload.into_inner().name)?);
    replace_host_group(
        &state,
        group.clone(),
        group.hosts().to_vec(),
        group.parent_groups().to_vec(),
        owners,
    )
    .await?;
    Ok(HttpResponse::Created().finish())
}

async fn host_group_owner_remove(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (name, member) = path.into_inner();
    let group = get_host_group(&req, &state, name).await?;
    let member = OwnerGroupName::new(member)?;
    let owners = group
        .owner_groups()
        .iter()
        .filter(|value| *value != &member)
        .cloned()
        .collect();
    replace_host_group(
        &state,
        group.clone(),
        group.hosts().to_vec(),
        group.parent_groups().to_vec(),
        owners,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn network_by_ip(
    req: HttpRequest,
    state: web::Data<AppState>,
    ip: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let ip = IpAddressValue::new(ip.into_inner())?;
    authorize(
        &req,
        &state,
        actions::network::LIST,
        actions::resource_kinds::NETWORK,
        &ip.as_str(),
    )
    .await?;
    let filter = NetworkFilter {
        contains_ip: Some(ip),
        ..Default::default()
    };
    let page = state
        .services
        .networks()
        .list(&PageRequest::all(), &filter)
        .await?;
    let network = page
        .items
        .first()
        .ok_or_else(|| AppError::not_found("network was not found"))?;
    Ok(HttpResponse::Ok().json(network_json(&state, network).await?))
}

async fn with_network(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    raw: String,
) -> Result<(CidrValue, crate::domain::network::Network), AppError> {
    let cidr = CidrValue::new(raw)?;
    authorize(
        req,
        state,
        actions::network::GET,
        actions::resource_kinds::NETWORK,
        &cidr.as_str(),
    )
    .await?;
    let network = state.services.networks().get(&cidr).await?;
    Ok((cidr, network))
}

pub(super) async fn network_json(
    state: &AppState,
    network: &crate::domain::network::Network,
) -> Result<Value, AppError> {
    let network_id = super::legacy_id(network.id());
    let excluded_ranges = state
        .services
        .networks()
        .list_excluded_ranges(network.cidr(), &PageRequest::all())
        .await?
        .items
        .iter()
        .map(|range| {
            json!({
                "id": super::legacy_id(range.id()), "network": network_id,
                "start_ip": range.start_ip().as_str(), "end_ip": range.end_ip().as_str(),
                "created_at": range.created_at(), "updated_at": range.updated_at(),
            })
        })
        .collect::<Vec<_>>();
    let communities = state
        .services
        .communities()
        .list(
            &PageRequest::all(),
            &crate::domain::filters::CommunityFilter::default(),
        )
        .await?
        .items
        .into_iter()
        .filter(|community| community.network_cidr() == network.cidr())
        .map(|community| {
            json!({
                "id": super::legacy_id(community.id()), "name": community.name().as_str(),
                "description": community.description(), "network": network_id,
                "created_at": community.created_at(), "updated_at": community.updated_at(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "id": super::legacy_id(network.id()), "network": network.cidr().as_str(),
        "description": network.description(), "vlan": network.vlan().map(|v| v.as_u32()),
        "dns_delegated": network.dns_delegated(), "category": network.category(),
        "location": network.location(), "frozen": network.frozen(),
        "reserved": network.reserved().as_u32().saturating_sub(1), "created_at": network.created_at(),
        "updated_at": network.updated_at(), "excluded_ranges": excluded_ranges,
        "policy": Value::Null, "communities": communities, "max_communities": Value::Null,
    }))
}

async fn network_first_unused(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (cidr, _) = with_network(&req, &state, network.into_inner()).await?;
    let value = state
        .services
        .networks()
        .list_unused_addresses(&cidr, Some(1))
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::not_found("No available IPs"))?;
    Ok(HttpResponse::Ok().json(value.as_str()))
}

async fn network_random_unused(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (cidr, _) = with_network(&req, &state, network.into_inner()).await?;
    let values = state
        .services
        .networks()
        .list_unused_addresses(&cidr, Some(4096))
        .await?;
    let value = values
        .choose(&mut rand::thread_rng())
        .ok_or_else(|| AppError::not_found("No available IPs"))?;
    Ok(HttpResponse::Ok().json(value.as_str()))
}

async fn all_ptrs(
    state: &web::Data<AppState>,
) -> Result<Vec<crate::domain::ptr_override::PtrOverride>, AppError> {
    Ok(state
        .services
        .ptr_overrides()
        .list(&PageRequest::all(), &PtrOverrideFilter::default())
        .await?
        .items)
}

async fn network_ptr_list(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (_, network) = with_network(&req, &state, network.into_inner()).await?;
    let mut values = all_ptrs(&state)
        .await?
        .into_iter()
        .filter(|item| network.contains(item.address()))
        .map(|item| item.address().as_str())
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.parse::<IpAddr>().ok());
    Ok(HttpResponse::Ok().json(values))
}

async fn network_ptr_host_list(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (_, network) = with_network(&req, &state, network.into_inner()).await?;
    let values = all_ptrs(&state)
        .await?
        .into_iter()
        .filter(|item| network.contains(item.address()))
        .map(|item| {
            (
                item.address().as_str(),
                item.host_name().as_str().to_string(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(HttpResponse::Ok().json(values))
}

async fn network_reserved_list(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (_, network) = with_network(&req, &state, network.into_inner()).await?;
    let count = network.reserved().as_u32() as u128;
    let values = match network.cidr().as_inner() {
        IpNet::V4(net) => {
            let start = u32::from(net.network()) as u128;
            let mut values = (start..start + count)
                .map(|raw| Ipv4Addr::from(raw as u32).to_string())
                .collect::<Vec<_>>();
            let broadcast = net.broadcast().to_string();
            if !values.contains(&broadcast) {
                values.push(broadcast);
            }
            values
        }
        IpNet::V6(net) => {
            let start = u128::from(net.network());
            (start..start + count)
                .map(|raw| Ipv6Addr::from(raw).to_string())
                .collect::<Vec<_>>()
        }
    };
    Ok(HttpResponse::Ok().json(values))
}

async fn used(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    raw: String,
) -> Result<Vec<IpAddressAssignment>, AppError> {
    let (cidr, _) = with_network(req, state, raw).await?;
    state.services.networks().list_used_addresses(&cidr).await
}

async fn network_used_count(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(used(&req, &state, network.into_inner()).await?.len()))
}

async fn network_used_list(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let values = used(&req, &state, network.into_inner())
        .await?
        .into_iter()
        .map(|item| item.address().as_str())
        .collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(values))
}

async fn host_names(state: &web::Data<AppState>) -> Result<HashMap<Uuid, String>, AppError> {
    let page = state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?;
    Ok(page
        .items
        .into_iter()
        .map(|host| (host.id(), host.name().as_str().to_string()))
        .collect())
}

async fn network_used_host_list(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let names = host_names(&state).await?;
    let mut values = BTreeMap::<String, Vec<String>>::new();
    for item in used(&req, &state, network.into_inner()).await? {
        if let Some(name) = names.get(&item.host_id()) {
            values
                .entry(item.address().as_str())
                .or_default()
                .push(name.clone());
        }
    }
    for names in values.values_mut() {
        names.sort();
    }
    Ok(HttpResponse::Ok().json(values))
}

async fn network_unused_count(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (cidr, _) = with_network(&req, &state, network.into_inner()).await?;
    Ok(HttpResponse::Ok().json(
        state
            .services
            .networks()
            .count_unused_addresses(&cidr)
            .await?,
    ))
}

async fn network_unused_list(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let (cidr, _) = with_network(&req, &state, network.into_inner()).await?;
    let values = state
        .services
        .networks()
        .list_unused_addresses(&cidr, Some(4096))
        .await?
        .into_iter()
        .map(|item| item.as_str())
        .collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(values))
}

async fn dhcp_hosts_v4(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    dhcp_hosts(&req, &state, "0.0.0.0/0").await
}

async fn dhcp_hosts_v6(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    dhcp_hosts(&req, &state, "::/0").await
}

async fn dhcp_hosts_range(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, u8)>,
) -> Result<HttpResponse, AppError> {
    let (ip, prefix) = path.into_inner();
    dhcp_hosts(&req, &state, &format!("{ip}/{prefix}")).await
}

async fn hosts_and_assignments(
    req: &HttpRequest,
    state: &web::Data<AppState>,
) -> Result<
    (
        HashMap<Uuid, (String, Option<String>)>,
        Vec<IpAddressAssignment>,
    ),
    AppError,
> {
    authorize(
        req,
        state,
        actions::host::LIST,
        actions::resource_kinds::HOST,
        "*",
    )
    .await?;
    let hosts = state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?
        .items
        .into_iter()
        .map(|host| {
            (
                host.id(),
                (
                    host.name().as_str().to_string(),
                    host.zone().map(|zone| zone.as_str().to_string()),
                ),
            )
        })
        .collect();
    let assignments = state
        .services
        .hosts()
        .list_ip_addresses(&PageRequest::all())
        .await?
        .items;
    Ok((hosts, assignments))
}

async fn dhcp_hosts(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    range: &str,
) -> Result<HttpResponse, AppError> {
    let range = CidrValue::new(range)?;
    let (hosts, assignments) = hosts_and_assignments(req, state).await?;
    let values = assignments
        .into_iter()
        .filter(|item| {
            range.as_inner().contains(&item.address().as_inner()) && item.mac_address().is_some()
        })
        .filter_map(|item| {
            hosts.get(&item.host_id()).map(|(name, zone)| {
                json!({
                    "host__name": name, "host__zone__name": zone,
                    "ipaddress": item.address().as_str(),
                    "macaddress": item.mac_address().map(|value| value.as_str()),
                })
            })
        })
        .collect::<Vec<_>>();
    Ok(HttpResponse::Ok().json(values))
}

async fn dhcp_v6_by_v4_all(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    dhcp_v6_by_v4(&req, &state, "0.0.0.0/0").await
}

async fn dhcp_v6_by_v4_range(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, u8)>,
) -> Result<HttpResponse, AppError> {
    let (ip, prefix) = path.into_inner();
    dhcp_v6_by_v4(&req, &state, &format!("{ip}/{prefix}")).await
}

async fn dhcp_v6_by_v4(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    range: &str,
) -> Result<HttpResponse, AppError> {
    let range = CidrValue::new(range)?;
    let (hosts, assignments) = hosts_and_assignments(req, state).await?;
    let mut grouped = HashMap::<Uuid, Vec<IpAddressAssignment>>::new();
    for item in assignments {
        grouped.entry(item.host_id()).or_default().push(item);
    }
    let mut values = Vec::new();
    for (host_id, items) in grouped {
        let v4 = items
            .iter()
            .filter(|item| item.family() == 4)
            .collect::<Vec<_>>();
        let v6 = items
            .iter()
            .filter(|item| item.family() == 6)
            .collect::<Vec<_>>();
        if v4.len() == 1
            && v6.len() == 1
            && range.as_inner().contains(&v4[0].address().as_inner())
            && v4[0].mac_address().is_some()
            && v6[0].mac_address().is_none()
            && let Some((name, zone)) = hosts.get(&host_id)
        {
            values.push(json!({
                "host__name": name, "host__zone__name": zone,
                "ipaddress": v6[0].address().as_str(),
                "macaddress": v4[0].mac_address().map(|value| value.as_str()),
            }));
        }
    }
    values.sort_by_key(|value| {
        value["ipaddress"]
            .as_str()
            .and_then(|value| value.parse::<IpAddr>().ok())
    });
    Ok(HttpResponse::Ok().json(values))
}

fn nameservers_json(values: impl Iterator<Item = String>) -> Vec<Value> {
    values.map(|name| json!({"name": name})).collect()
}

pub(super) async fn forward_zone_json(state: &AppState, zone: &ForwardZone) -> Value {
    let mut nameservers = Vec::new();
    for name in zone.nameservers() {
        let value = match state.services.nameservers().get(name).await {
            Ok(nameserver) => json!({
                "id": super::legacy_id(nameserver.id()),
                "created_at": nameserver.created_at(), "updated_at": nameserver.updated_at(),
                "name": nameserver.name().as_str(),
                "ttl": nameserver.ttl().map(|value| value.as_u32()),
            }),
            Err(_) => json!({
                "id": super::legacy_id(zone.id()),
                "created_at": zone.created_at(), "updated_at": zone.updated_at(),
                "name": name.as_str(), "ttl": Value::Null,
            }),
        };
        nameservers.push(value);
    }
    json!({
        "id": super::legacy_id(zone.id()), "name": zone.name().as_str(), "updated": zone.updated(),
        "primary_ns": zone.primary_ns().as_str(),
        "nameservers": nameservers,
        "email": zone.email().as_str(), "serialno": zone.serial_no().as_u64(),
        "serialno_updated_at": zone.serial_no_updated_at(), "refresh": zone.refresh().as_u32(),
        "retry": zone.retry().as_u32(), "expire": zone.expire().as_u32(),
        "soa_ttl": zone.soa_ttl().as_u32(), "default_ttl": zone.default_ttl().as_u32(),
        "created_at": zone.created_at(), "updated_at": zone.updated_at(),
    })
}

async fn forward_zone_by_hostname(
    req: HttpRequest,
    state: web::Data<AppState>,
    hostname: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let hostname = hostname.into_inner().to_ascii_lowercase();
    authorize(
        &req,
        &state,
        actions::zone::forward::LIST,
        actions::resource_kinds::FORWARD_ZONE,
        &hostname,
    )
    .await?;
    let zones = state
        .services
        .zones()
        .list_forward(&PageRequest::all())
        .await?;
    let zone = zones
        .items
        .into_iter()
        .filter(|zone| {
            hostname == zone.name().as_str()
                || hostname.ends_with(&format!(".{}", zone.name().as_str()))
        })
        .max_by_key(|zone| zone.name().as_str().len())
        .ok_or_else(|| AppError::not_found("zone was not found"))?;
    let delegations = state
        .services
        .zones()
        .list_forward_delegations(zone.name(), &PageRequest::all())
        .await?;
    if let Some(delegation) = delegations
        .items
        .into_iter()
        .filter(|delegation| {
            hostname == delegation.name().as_str()
                || hostname.ends_with(&format!(".{}", delegation.name().as_str()))
        })
        .max_by_key(|delegation| delegation.name().as_str().len())
    {
        return Ok(HttpResponse::Ok()
            .json(json!({"delegation": forward_delegation_json(&state, &delegation).await})));
    }
    Ok(HttpResponse::Ok().json(json!({"zone": forward_zone_json(&state, &zone).await})))
}

async fn forward_zone_nameservers(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::zone::forward::GET,
        actions::resource_kinds::FORWARD_ZONE,
        name.as_str(),
    )
    .await?;
    let zone = state.services.zones().get_forward(&name).await?;
    Ok(HttpResponse::Ok().json(
        zone.nameservers()
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
    ))
}

async fn reverse_zone_nameservers(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::zone::reverse::GET,
        actions::resource_kinds::REVERSE_ZONE,
        name.as_str(),
    )
    .await?;
    let zone = state.services.zones().get_reverse(&name).await?;
    Ok(HttpResponse::Ok().json(
        zone.nameservers()
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
    ))
}

async fn forward_delegation_json(state: &AppState, value: &ForwardZoneDelegation) -> Value {
    let mut nameservers = Vec::new();
    for name in value.nameservers() {
        nameservers.push(match state.services.nameservers().get(name).await {
            Ok(item) => json!({
                "id": super::legacy_id(item.id()),
                "created_at": item.created_at(), "updated_at": item.updated_at(),
                "name": item.name().as_str(), "ttl": item.ttl().map(|ttl| ttl.as_u32()),
            }),
            Err(_) => json!({"name": name.as_str(), "ttl": Value::Null}),
        });
    }
    json!({
        "id": super::legacy_id(value.id()), "zone": super::legacy_id(value.zone_id()), "name": value.name().as_str(),
        "comment": value.comment(),
        "nameservers": nameservers,
        "created_at": value.created_at(), "updated_at": value.updated_at(),
    })
}

fn reverse_delegation_json(value: &ReverseZoneDelegation) -> Value {
    json!({
        "id": value.id(), "zone": value.zone_id(), "name": value.name().as_str(),
        "comment": value.comment(),
        "nameservers": nameservers_json(value.nameservers().iter().map(|item| item.as_str().to_string())),
        "created_at": value.created_at(), "updated_at": value.updated_at(),
    })
}

async fn forward_delegation_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (zone, delegation) = path.into_inner();
    let zone = ZoneName::new(zone)?;
    authorize(
        &req,
        &state,
        actions::zone::forward::delegation::LIST,
        actions::resource_kinds::FORWARD_ZONE_DELEGATION,
        &delegation,
    )
    .await?;
    let item = state
        .services
        .zones()
        .list_forward_delegations(&zone, &PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|item| item.name().as_str() == delegation)
        .ok_or_else(|| AppError::not_found("delegation was not found"))?;
    Ok(HttpResponse::Ok().json(forward_delegation_json(&state, &item).await))
}

#[derive(Deserialize)]
struct LegacyDelegationRequest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    nameservers: Vec<String>,
}

async fn forward_delegations(
    req: HttpRequest,
    state: web::Data<AppState>,
    zone: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let zone = ZoneName::new(zone.into_inner())?;
    authorize(
        &req,
        &state,
        actions::zone::forward::delegation::LIST,
        actions::resource_kinds::FORWARD_ZONE_DELEGATION,
        zone.as_str(),
    )
    .await?;
    let items = state
        .services
        .zones()
        .list_forward_delegations(&zone, &PageRequest::all())
        .await?
        .items;
    let mut values = Vec::with_capacity(items.len());
    for item in &items {
        values.push(forward_delegation_json(&state, item).await);
    }
    Ok(HttpResponse::Ok().json(legacy_page(values)))
}

async fn create_forward_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    zone: web::Path<String>,
    payload: web::Json<LegacyDelegationRequest>,
) -> Result<HttpResponse, AppError> {
    let zone = ZoneName::new(zone.into_inner())?;
    let payload = payload.into_inner();
    authorize(
        &req,
        &state,
        actions::zone::forward::delegation::CREATE,
        actions::resource_kinds::FORWARD_ZONE_DELEGATION,
        &payload.name,
    )
    .await?;
    let nameservers = payload
        .nameservers
        .into_iter()
        .map(DnsName::new)
        .collect::<Result<Vec<_>, _>>()?;
    state
        .services
        .zones()
        .create_forward_delegation(CreateForwardZoneDelegation::new(
            zone,
            DnsName::new(payload.name)?,
            payload.comment,
            nameservers,
        ))
        .await?;
    Ok(HttpResponse::Created().finish())
}

async fn find_forward_delegation(
    state: &AppState,
    zone: &ZoneName,
    name: &str,
) -> Result<ForwardZoneDelegation, AppError> {
    state
        .services
        .zones()
        .list_forward_delegations(zone, &PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|item| item.name().as_str() == name)
        .ok_or_else(|| AppError::not_found("delegation was not found"))
}

async fn update_forward_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    payload: web::Json<LegacyDelegationRequest>,
) -> Result<HttpResponse, AppError> {
    let (zone, name) = path.into_inner();
    let zone = ZoneName::new(zone)?;
    authorize(
        &req,
        &state,
        actions::zone::forward::delegation::DELETE,
        actions::resource_kinds::FORWARD_ZONE_DELEGATION,
        &name,
    )
    .await?;
    let old = find_forward_delegation(&state, &zone, &name).await?;
    let payload = payload.into_inner();
    let comment = payload.comment;
    let nameservers = if payload.nameservers.is_empty() {
        old.nameservers().to_vec()
    } else {
        payload
            .nameservers
            .into_iter()
            .map(DnsName::new)
            .collect::<Result<Vec<_>, _>>()?
    };
    state
        .services
        .zones()
        .delete_forward_delegation(old.id())
        .await?;
    state
        .services
        .zones()
        .create_forward_delegation(CreateForwardZoneDelegation::new(
            zone,
            old.name().clone(),
            comment,
            nameservers,
        ))
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_forward_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (zone, name) = path.into_inner();
    let zone = ZoneName::new(zone)?;
    authorize(
        &req,
        &state,
        actions::zone::forward::delegation::DELETE,
        actions::resource_kinds::FORWARD_ZONE_DELEGATION,
        &name,
    )
    .await?;
    let item = find_forward_delegation(&state, &zone, &name).await?;
    state
        .services
        .zones()
        .delete_forward_delegation(item.id())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn reverse_delegation_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (zone, delegation) = path.into_inner();
    let zone = ZoneName::new(zone)?;
    authorize(
        &req,
        &state,
        actions::zone::reverse::delegation::LIST,
        actions::resource_kinds::REVERSE_ZONE_DELEGATION,
        &delegation,
    )
    .await?;
    let item = state
        .services
        .zones()
        .list_reverse_delegations(&zone, &PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|item| item.name().as_str() == delegation)
        .ok_or_else(|| AppError::not_found("delegation was not found"))?;
    Ok(HttpResponse::Ok().json(reverse_delegation_json(&item)))
}

async fn host_policy_role_atoms(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::GET,
        actions::resource_kinds::HOST_POLICY_ROLE,
        name.as_str(),
    )
    .await?;
    let role = state.services.host_policy().get_role(&name).await?;
    Ok(HttpResponse::Ok().json(legacy_page(
        role.atoms()
            .iter()
            .map(|name| json!({"name": name}))
            .collect(),
    )))
}

async fn host_policy_role_atom_add(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyNamedMember>,
) -> Result<HttpResponse, AppError> {
    let role = HostPolicyName::new(name.into_inner())?;
    let atom = HostPolicyName::new(payload.into_inner().name)?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::ATOM_ATTACH,
        actions::resource_kinds::HOST_POLICY_ROLE,
        role.as_str(),
    )
    .await?;
    state
        .services
        .host_policy()
        .add_atom_to_role(&role, &atom)
        .await?;
    Ok(HttpResponse::Created().finish())
}

async fn host_policy_role_atom_remove(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role, atom) = path.into_inner();
    let role = HostPolicyName::new(role)?;
    let atom = HostPolicyName::new(atom)?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::ATOM_DETACH,
        actions::resource_kinds::HOST_POLICY_ROLE,
        role.as_str(),
    )
    .await?;
    state
        .services
        .host_policy()
        .remove_atom_from_role(&role, &atom)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn host_policy_role_hosts(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::GET,
        actions::resource_kinds::HOST_POLICY_ROLE,
        name.as_str(),
    )
    .await?;
    let role = state.services.host_policy().get_role(&name).await?;
    Ok(HttpResponse::Ok().json(legacy_page(
        role.hosts()
            .iter()
            .map(|name| json!({"name": name}))
            .collect(),
    )))
}

async fn host_policy_role_host_add(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyNamedMember>,
) -> Result<HttpResponse, AppError> {
    let role = HostPolicyName::new(name.into_inner())?;
    let host = Hostname::new(payload.into_inner().name)?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::HOST_ATTACH,
        actions::resource_kinds::HOST_POLICY_ROLE,
        role.as_str(),
    )
    .await?;
    state
        .services
        .host_policy()
        .add_host_to_role(&role, host.as_str())
        .await?;
    Ok(HttpResponse::Created().finish())
}

async fn host_policy_role_host_remove(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role, host) = path.into_inner();
    let role = HostPolicyName::new(role)?;
    let host = Hostname::new(host)?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::HOST_DETACH,
        actions::resource_kinds::HOST_POLICY_ROLE,
        role.as_str(),
    )
    .await?;
    state
        .services
        .host_policy()
        .remove_host_from_role(&role, host.as_str())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn zone_file(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::record::LIST,
        actions::resource_kinds::RECORD,
        name.as_str(),
    )
    .await?;
    let forward = state
        .services
        .zones()
        .list_forward(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|zone| zone.name() == &name);
    let reverse = state
        .services
        .zones()
        .list_reverse(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|zone| zone.name() == &name);
    let zone_id = forward
        .as_ref()
        .map(ForwardZone::id)
        .or_else(|| reverse.as_ref().map(|zone| zone.id()))
        .ok_or_else(|| AppError::not_found("zone was not found"))?;
    let records = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items;

    let (primary_ns, email, serial, refresh, retry, expire, soa_ttl, default_ttl, reverse_mode) =
        if let Some(zone) = forward.as_ref() {
            (
                zone.primary_ns().as_str(),
                zone.email().as_str(),
                zone.serial_no().as_u64(),
                zone.refresh().as_u32(),
                zone.retry().as_u32(),
                zone.expire().as_u32(),
                zone.soa_ttl().as_u32(),
                zone.default_ttl().as_u32(),
                false,
            )
        } else {
            let zone = reverse.as_ref().expect("zone existence checked above");
            (
                zone.primary_ns().as_str(),
                zone.email().as_str(),
                zone.serial_no().as_u64(),
                zone.refresh().as_u32(),
                zone.retry().as_u32(),
                zone.expire().as_u32(),
                zone.soa_ttl().as_u32(),
                zone.default_ttl().as_u32(),
                true,
            )
        };
    let mut output = format!(
        "; {}zone file for {}\n; Generated by mreg-rust\n$ORIGIN {}.\n$TTL {}\n\n@ {} IN SOA {}. {}. (\n    {}\n    {}\n    {}\n    {}\n    {}\n)\n\n",
        if reverse_mode { "Reverse " } else { "" },
        name.as_str(),
        name.as_str(),
        default_ttl,
        soa_ttl,
        primary_ns,
        email.replace('@', "."),
        serial,
        refresh,
        retry,
        expire,
        soa_ttl,
    );
    for record in records
        .iter()
        .filter(|record| record.zone_id() == Some(zone_id))
    {
        let owner = if !reverse_mode && record.owner_name() == name.as_str() {
            "@".to_string()
        } else {
            format!("{}.", record.owner_name())
        };
        let ttl = record
            .ttl()
            .map(|value| value.as_u32())
            .unwrap_or(default_ttl);
        let rendered = record
            .rendered()
            .map(str::to_string)
            .or_else(|| record.raw_rdata().map(|value| value.presentation()))
            .unwrap_or_default();
        output.push_str(&format!(
            "{owner} {ttl} IN {} {rendered}\n",
            record.type_name().as_str()
        ));
    }
    Ok(HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(output))
}
