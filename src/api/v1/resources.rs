//! Legacy collection and detail response adapters.

use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    AppState,
    authz::actions,
    domain::{
        filters::{
            BacnetIdFilter, CommunityFilter, HostFilter, HostGroupFilter, NetworkFilter,
            NetworkPolicyFilter, PtrOverrideFilter, RecordFilter,
        },
        host::{
            AllocationPolicy, AssignIpAddress, CreateHost, IpAssignmentSpec, UpdateHost,
            UpdateIpAddress,
        },
        host_contact::CreateHostContact,
        host_group::CreateHostGroup,
        host_policy::{
            CreateHostPolicyAtom, CreateHostPolicyRole, HostPolicyRole, UpdateHostPolicyAtom,
            UpdateHostPolicyRole,
        },
        label::{CreateLabel, UpdateLabel},
        nameserver::CreateNameServer,
        network::{CreateExcludedRange, CreateNetwork, UpdateNetwork},
        network_policy::{
            CreateNetworkPolicy, CreateNetworkPolicyAttribute, NetworkPolicy,
            NetworkPolicyAttribute, NetworkPolicyDetails, SetNetworkPolicyAttributeValue,
            UpdateNetworkPolicy, UpdateNetworkPolicyAttribute,
        },
        pagination::PageRequest,
        ptr_override::CreatePtrOverride,
        resource_records::{CreateRecordInstance, RecordInstance, RecordOwnerKind},
        types::{
            BacnetIdentifier, CidrValue, DnsName, EmailAddressValue, HostGroupName, HostPolicyName,
            Hostname, IpAddressValue, LabelName, MacAddressValue, NetworkPolicyAttributeName,
            NetworkPolicyName, RecordTypeName, ReservedCount, SerialNumber, SoaSeconds, Ttl,
            UpdateField, VlanId, ZoneName,
        },
        zone::{CreateForwardZone, UpdateForwardZone},
    },
    errors::AppError,
};

use super::{legacy_id, legacy_name_id, reads::authorize};

#[derive(Clone, Deserialize)]
struct LegacyPageQuery {
    #[serde(default = "default_page")]
    page: usize,
    #[serde(default = "default_page_size")]
    page_size: usize,
    id: Option<u32>,
    #[serde(rename = "id__in")]
    ids: Option<String>,
    network: Option<String>,
    #[serde(rename = "description__regex")]
    description_regex: Option<String>,
    vlan: Option<u32>,
    dns_delegated: Option<u8>,
    category: Option<String>,
    location: Option<String>,
    frozen: Option<u8>,
    reserved: Option<u32>,
    host: Option<u32>,
    attributes: Option<u32>,
    #[serde(rename = "attributes__name")]
    attributes_name: Option<String>,
    #[serde(rename = "attributes__description")]
    attributes_description: Option<String>,
    ipaddress: Option<String>,
    #[serde(rename = "ipaddresses__ipaddress")]
    host_ipaddress: Option<String>,
    #[serde(rename = "ptr_overrides__ipaddress")]
    host_ptr_address: Option<String>,
    macaddress: Option<String>,
    resource: Option<String>,
    name: Option<String>,
    zone: Option<u32>,
    #[serde(rename = "comment__regex")]
    comment_regex: Option<String>,
    #[serde(rename = "contact__regex")]
    contact_regex: Option<String>,
    cpu: Option<String>,
    os: Option<String>,
    loc: Option<String>,
    mx: Option<String>,
    priority: Option<String>,
    order: Option<String>,
    preference: Option<String>,
    flag: Option<String>,
    service: Option<String>,
    regex: Option<String>,
    replacement: Option<String>,
    algorithm: Option<String>,
    hash_type: Option<String>,
    fingerprint: Option<String>,
    port: Option<String>,
    weight: Option<String>,
    ttl: Option<String>,
    txt: Option<String>,
    #[serde(rename = "name__regex")]
    name_regex: Option<String>,
    #[serde(rename = "atoms__name__exact")]
    atoms_name_exact: Option<String>,
    #[serde(rename = "model_id__in")]
    model_ids: Option<String>,
    #[serde(rename = "data__relation")]
    data_relation: Option<String>,
    #[serde(rename = "data__id__in")]
    data_ids: Option<String>,
}

const fn default_page() -> usize {
    1
}
const fn default_page_size() -> usize {
    100
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/labels/")
            .route(web::get().to(labels))
            .route(web::post().to(create_label)),
    )
    .route("/labels/name/{name}", web::get().to(label_by_name))
    .service(
        web::resource("/labels/{id}")
            .route(web::get().to(label_detail))
            .route(web::patch().to(update_label))
            .route(web::delete().to(delete_label)),
    )
    .route("/nameservers/", web::get().to(nameservers))
    .route("/nameservers/{name}", web::get().to(nameserver_detail))
    .route("/bacnet/ids/", web::get().to(bacnet_ids))
    .route("/bacnet/ids/{id}", web::get().to(bacnet_detail))
    .service(
        web::resource("/hosts/")
            .route(web::get().to(hosts))
            .route(web::post().to(create_host)),
    )
    .service(
        web::resource("/hosts/{name}")
            .route(web::get().to(host_detail))
            .route(web::patch().to(update_host))
            .route(web::delete().to(delete_host)),
    )
    .service(
        web::resource("/ipaddresses/")
            .route(web::get().to(ip_addresses))
            .route(web::post().to(create_ip_address)),
    )
    .service(
        web::resource("/ipaddresses/{id}")
            .route(web::get().to(ip_address_detail))
            .route(web::patch().to(update_ip_address))
            .route(web::delete().to(delete_ip_address)),
    )
    .service(
        web::resource("/hostgroups/")
            .route(web::get().to(host_groups))
            .route(web::post().to(create_host_group)),
    )
    .service(
        web::resource("/hostgroups/{name}")
            .route(web::get().to(host_group_detail))
            .route(web::delete().to(delete_host_group)),
    )
    .service(
        web::resource("/ptroverrides/")
            .route(web::get().to(ptr_overrides))
            .route(web::post().to(create_ptr_override)),
    )
    .service(
        web::resource("/ptroverrides/{id}")
            .route(web::patch().to(update_ptr_override))
            .route(web::delete().to(delete_ptr_override)),
    )
    .service(
        web::resource("/networks/")
            .route(web::get().to(networks))
            .route(web::post().to(create_network)),
    )
    .service(
        web::resource("/networks/{network:.*}/excluded_ranges/")
            .route(web::post().to(create_excluded_range)),
    )
    .service(
        web::resource("/networks/{network:.*}/excluded_ranges/{id}")
            .route(web::delete().to(delete_excluded_range)),
    )
    .service(
        web::resource("/networks/{network:.*}")
            .route(web::get().to(network_detail))
            .route(web::patch().to(update_network))
            .route(web::delete().to(delete_network)),
    )
    .service(
        web::resource("/zones/forward/")
            .route(web::get().to(forward_zones))
            .route(web::post().to(create_forward_zone)),
    )
    .service(
        web::resource("/zones/forward/{name}/nameservers")
            .route(web::patch().to(update_forward_zone_nameservers)),
    )
    .service(
        web::resource("/zones/forward/{name:.*}")
            .route(web::get().to(forward_zone_detail))
            .route(web::patch().to(update_forward_zone))
            .route(web::delete().to(delete_forward_zone)),
    )
    .route("/zones/reverse/", web::get().to(reverse_zones))
    .route(
        "/zones/reverse/{name:.*}",
        web::get().to(reverse_zone_detail),
    )
    .service(
        web::resource("/networkpolicies/")
            .route(web::get().to(network_policies))
            .route(web::post().to(create_network_policy)),
    )
    .service(
        web::resource("/networkpolicies/{id}")
            .route(web::get().to(network_policy_detail))
            .route(web::patch().to(update_network_policy))
            .route(web::put().to(update_network_policy))
            .route(web::delete().to(delete_network_policy)),
    )
    .service(
        web::resource("/networkpolicyattributes/")
            .route(web::get().to(network_policy_attributes))
            .route(web::post().to(create_network_policy_attribute)),
    )
    .service(
        web::resource("/networkpolicyattributes/{id}")
            .route(web::get().to(network_policy_attribute_detail))
            .route(web::patch().to(update_network_policy_attribute))
            .route(web::put().to(update_network_policy_attribute))
            .route(web::delete().to(delete_network_policy_attribute)),
    )
    .service(
        web::resource("/hostpolicy/atoms/")
            .route(web::get().to(host_policy_atoms))
            .route(web::post().to(create_host_policy_atom)),
    )
    .service(
        web::resource("/hostpolicy/atoms/{name}")
            .route(web::get().to(host_policy_atom_detail))
            .route(web::patch().to(update_host_policy_atom))
            .route(web::delete().to(delete_host_policy_atom)),
    )
    .service(
        web::resource("/hostpolicy/roles/")
            .route(web::get().to(host_policy_roles))
            .route(web::post().to(create_host_policy_role)),
    )
    .service(
        web::resource("/hostpolicy/roles/{name}")
            .route(web::get().to(host_policy_role_detail))
            .route(web::patch().to(update_host_policy_role))
            .route(web::delete().to(delete_host_policy_role)),
    )
    .route("/history/", web::get().to(history))
    .service(
        web::resource("/cnames/")
            .route(web::get().to(cnames))
            .route(web::post().to(create_cname)),
    )
    .service(
        web::resource("/cnames/{name}")
            .route(web::get().to(cname_detail))
            .route(web::delete().to(delete_cname)),
    )
    .service(
        web::resource("/hinfos/")
            .route(web::get().to(hinfos))
            .route(web::post().to(create_hinfo)),
    )
    .service(
        web::resource("/hinfos/{id}")
            .route(web::get().to(hinfo_detail))
            .route(web::delete().to(delete_hinfo)),
    )
    .service(
        web::resource("/locs/")
            .route(web::get().to(locs))
            .route(web::post().to(create_loc)),
    )
    .service(web::resource("/locs/{id}").route(web::delete().to(delete_loc)))
    .service(
        web::resource("/mxs/")
            .route(web::get().to(mxs))
            .route(web::post().to(create_mx)),
    )
    .service(web::resource("/mxs/{id}").route(web::delete().to(delete_mx)))
    .service(
        web::resource("/naptrs/")
            .route(web::get().to(naptrs))
            .route(web::post().to(create_naptr)),
    )
    .service(web::resource("/naptrs/{id}").route(web::delete().to(delete_naptr)))
    .service(
        web::resource("/sshfps/")
            .route(web::get().to(sshfps))
            .route(web::post().to(create_sshfp)),
    )
    .service(web::resource("/sshfps/{id}").route(web::delete().to(delete_sshfp)))
    .service(
        web::resource("/srvs/")
            .route(web::get().to(srvs))
            .route(web::post().to(create_srv)),
    )
    .service(web::resource("/srvs/{id}").route(web::delete().to(delete_srv)))
    .service(
        web::resource("/txts/")
            .route(web::get().to(txts))
            .route(web::post().to(create_txt)),
    )
    .service(web::resource("/txts/{id}").route(web::delete().to(delete_txt)))
    .route(
        "/networks/{network:.*}/communities/",
        web::get().to(network_communities),
    );
}

#[derive(Deserialize)]
struct LegacyCreateHostGroup {
    name: String,
    description: String,
}

async fn create_host_group(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateHostGroup>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = HostGroupName::new(payload.name)?;
    authorize(
        &req,
        &state,
        actions::host_group::CREATE,
        actions::resource_kinds::HOST_GROUP,
        name.as_str(),
    )
    .await?;
    state
        .services
        .host_groups()
        .create(CreateHostGroup::new(
            name.clone(),
            payload.description,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?)
        .await?;
    Ok(HttpResponse::Created()
        .append_header(("Location", format!("/api/v1/hostgroups/{}", name.as_str())))
        .finish())
}

async fn delete_host_group(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostGroupName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_group::DELETE,
        actions::resource_kinds::HOST_GROUP,
        name.as_str(),
    )
    .await?;
    state.services.host_groups().delete(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
struct LegacyCreateIpAddress {
    ipaddress: String,
    host: String,
    macaddress: Option<String>,
}

async fn find_ip_by_legacy_id(
    state: &AppState,
    id: u32,
) -> Result<crate::domain::host::IpAddressAssignment, AppError> {
    state
        .services
        .hosts()
        .list_ip_addresses(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|assignment| legacy_ip_id(assignment) == id)
        .ok_or_else(|| AppError::not_found("IP address was not found"))
}

async fn create_ip_address(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateIpAddress>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let host_id = payload
        .host
        .parse::<u32>()
        .map_err(|_| AppError::validation("host must be a legacy integer ID"))?;
    let host = state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?
        .items
        .into_iter()
        .find(|host| legacy_id(host.id()) == host_id)
        .ok_or_else(|| AppError::not_found("host was not found"))?;
    let address = IpAddressValue::new(payload.ipaddress)?;
    authorize(
        &req,
        &state,
        actions::host::ip::ASSIGN_MANUAL,
        actions::resource_kinds::IP_ADDRESS,
        &address.as_str(),
    )
    .await?;
    let mac_address = payload
        .macaddress
        .filter(|value| !value.is_empty())
        .map(MacAddressValue::new)
        .transpose()?;
    let assignment = state
        .services
        .hosts()
        .assign_ip_address(
            AssignIpAddress::new(host.name().clone(), Some(address), None, mac_address)?
                .with_reserved_addresses(true),
        )
        .await?;
    Ok(HttpResponse::Created().json(ip_json(&assignment, Some(legacy_id(assignment.host_id())))))
}

async fn ip_address_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let assignment = find_ip_by_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::host::ip::LIST,
        actions::resource_kinds::IP_ADDRESS,
        &assignment.address().as_str(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ip_json(&assignment, Some(legacy_id(assignment.host_id())))))
}

#[derive(Deserialize, Default)]
struct LegacyUpdateIpAddress {
    macaddress: Option<String>,
    ipaddress: Option<String>,
    host: Option<u32>,
}

async fn update_ip_address(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
    payload: web::Json<LegacyUpdateIpAddress>,
) -> Result<HttpResponse, AppError> {
    let assignment = find_ip_by_legacy_id(&state, id.into_inner()).await?;
    let payload = payload.into_inner();
    if payload.ipaddress.is_some() || payload.host.is_some() {
        let hosts = state
            .services
            .hosts()
            .list(&PageRequest::all(), &HostFilter::default())
            .await?
            .items;
        let host = hosts
            .into_iter()
            .find(|host| {
                payload
                    .host
                    .map_or(host.id() == assignment.host_id(), |id| {
                        legacy_id(host.id()) == id
                    })
            })
            .ok_or_else(|| AppError::not_found("host was not found"))?;
        let new_address = payload
            .ipaddress
            .map(IpAddressValue::new)
            .transpose()?
            .unwrap_or(*assignment.address());
        authorize(
            &req,
            &state,
            actions::host::ip::ASSIGN_MANUAL,
            actions::resource_kinds::IP_ADDRESS,
            &new_address.as_str(),
        )
        .await?;
        let mac_address = match payload.macaddress {
            Some(value) if !value.is_empty() => Some(MacAddressValue::new(value)?),
            Some(_) => None,
            None => assignment.mac_address().cloned(),
        };
        state
            .services
            .hosts()
            .unassign_ip_address(assignment.address())
            .await?;
        let replacement =
            AssignIpAddress::new(host.name().clone(), Some(new_address), None, mac_address)?
                .with_reserved_addresses(true)
                .with_assignment_id(assignment.id());
        if let Err(error) = state.services.hosts().assign_ip_address(replacement).await {
            let original_host = state
                .services
                .hosts()
                .list(&PageRequest::all(), &HostFilter::default())
                .await?
                .items
                .into_iter()
                .find(|host| host.id() == assignment.host_id())
                .ok_or_else(|| AppError::not_found("original host was not found"))?;
            state
                .services
                .hosts()
                .assign_ip_address(
                    AssignIpAddress::new(
                        original_host.name().clone(),
                        Some(*assignment.address()),
                        None,
                        assignment.mac_address().cloned(),
                    )?
                    .with_reserved_addresses(true)
                    .with_assignment_id(assignment.id()),
                )
                .await?;
            return Err(error);
        }
        return Ok(HttpResponse::NoContent().finish());
    }
    authorize(
        &req,
        &state,
        actions::host::ip::UPDATE_MAC,
        actions::resource_kinds::IP_ADDRESS,
        &assignment.address().as_str(),
    )
    .await?;
    state
        .services
        .hosts()
        .update_ip_address(
            assignment.address(),
            UpdateIpAddress {
                mac_address: match payload.macaddress {
                    Some(value) if value.is_empty() => UpdateField::Clear,
                    Some(value) => UpdateField::Set(MacAddressValue::new(value)?),
                    None => UpdateField::Unchanged,
                },
            },
        )
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_ip_address(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let assignment = find_ip_by_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::host::ip::UNASSIGN,
        actions::resource_kinds::IP_ADDRESS,
        &assignment.address().as_str(),
    )
    .await?;
    state
        .services
        .hosts()
        .unassign_ip_address(assignment.address())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
struct LegacyCreateExcludedRange {
    #[serde(rename = "network")]
    _network_id: u32,
    start_ip: String,
    end_ip: String,
}

async fn create_excluded_range(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
    payload: web::Json<LegacyCreateExcludedRange>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(network.into_inner())?;
    authorize(
        &req,
        &state,
        actions::network::EXCLUDED_RANGE_CREATE,
        actions::resource_kinds::NETWORK,
        &cidr.as_str(),
    )
    .await?;
    let payload = payload.into_inner();
    let range = state
        .services
        .networks()
        .add_excluded_range(
            &cidr,
            CreateExcludedRange::new(
                IpAddressValue::new(payload.start_ip)?,
                IpAddressValue::new(payload.end_ip)?,
                "legacy excluded range",
            )?,
        )
        .await?;
    Ok(HttpResponse::Created()
        .append_header((
            "Location",
            format!(
                "/api/v1/networks/{}/excluded_ranges/{}",
                cidr.as_str(),
                legacy_id(range.id())
            ),
        ))
        .finish())
}

async fn delete_excluded_range(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, u32)>,
) -> Result<HttpResponse, AppError> {
    let (network, id) = path.into_inner();
    let cidr = CidrValue::new(network)?;
    authorize(
        &req,
        &state,
        actions::network::EXCLUDED_RANGE_CREATE,
        actions::resource_kinds::NETWORK,
        &cidr.as_str(),
    )
    .await?;
    let range = state
        .services
        .networks()
        .list_excluded_ranges(&cidr, &PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|range| legacy_id(range.id()) == id)
        .ok_or_else(|| AppError::not_found("excluded range was not found"))?;
    state
        .services
        .networks()
        .delete_excluded_range(&cidr, &range)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
struct LegacyCreateHost {
    name: String,
    #[serde(default)]
    contacts: Vec<String>,
    network: Option<String>,
    ipaddress: Option<String>,
    #[serde(default)]
    comment: String,
    ttl: Option<u32>,
}

async fn create_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateHost>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = Hostname::new(payload.name)?;
    authorize(
        &req,
        &state,
        actions::host::CREATE,
        actions::resource_kinds::HOST,
        name.as_str(),
    )
    .await?;
    let mut zone = state
        .services
        .zones()
        .list_forward(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .filter(|zone| {
            name.as_str() == zone.name().as_str()
                || name
                    .as_str()
                    .ends_with(&format!(".{}", zone.name().as_str()))
        })
        .max_by_key(|zone| zone.name().as_str().len())
        .map(|zone| zone.name().clone());
    if let Some(zone_name) = zone.as_ref() {
        let delegated = state
            .services
            .zones()
            .list_forward_delegations(zone_name, &PageRequest::all())
            .await?
            .items
            .iter()
            .any(|delegation| {
                name.as_str() == delegation.name().as_str()
                    || name
                        .as_str()
                        .ends_with(&format!(".{}", delegation.name().as_str()))
            });
        if delegated {
            zone = None;
        }
    }
    let create_default_spf = zone.is_some();
    let assignment = match (payload.ipaddress, payload.network) {
        (None, None) => None,
        (address, network) => Some(IpAssignmentSpec::new(
            address.map(IpAddressValue::new).transpose()?,
            network.map(CidrValue::new).transpose()?,
            AllocationPolicy::Random,
            None,
        )?),
    };
    if let Some(spec) = assignment.as_ref()
        && let Some(address) = spec.address()
    {
        let filter = NetworkFilter {
            contains_ip: Some(*address),
            ..NetworkFilter::default()
        };
        if let Some(network) = state
            .services
            .networks()
            .list(&PageRequest::all(), &filter)
            .await?
            .items
            .first()
        {
            for range in state
                .services
                .networks()
                .list_excluded_ranges(network.cidr(), &PageRequest::all())
                .await?
                .items
            {
                if range.contains(address) {
                    return Ok(HttpResponse::BadRequest().json(json!({
                        "type": "validation_error",
                        "errors": [{
                            "code": "invalid",
                            "detail": format!(
                                "IP {} in an excluded range: {} -> [{} -> [{}]",
                                address.as_str(), network.cidr().as_str(),
                                range.start_ip().as_str(), range.end_ip().as_str()
                            ),
                            "attr": "non_field_errors"
                        }]
                    })));
                }
            }
        }
    }
    let command = CreateHost::new(
        name.clone(),
        zone,
        payload.ttl.map(Ttl::new).transpose()?,
        payload.comment,
    )?;
    state.services.hosts().create(command).await?;
    if let Some(spec) = assignment {
        let explicit = spec.address().is_some();
        let command = spec
            .into_assign_command(name.clone())?
            .with_reserved_addresses(explicit);
        if let Err(error) = state.services.hosts().assign_ip_address(command).await {
            let _ = state.services.hosts().delete(&name).await;
            return Err(error);
        }
    }
    if create_default_spf {
        state
            .services
            .records()
            .create_record(CreateRecordInstance::new(
                RecordTypeName::new("TXT")?,
                RecordOwnerKind::Host,
                name.as_str(),
                None,
                json!({"value": "v=spf1 -all"}),
            )?)
            .await?;
    }
    for email in payload.contacts {
        state
            .services
            .host_contacts()
            .create(CreateHostContact::new(
                EmailAddressValue::new(email)?,
                None,
                vec![name.clone()],
            ))
            .await?;
    }
    Ok(HttpResponse::Created()
        .append_header(("Location", format!("/api/v1/hosts/{}", name.as_str())))
        .finish())
}

async fn delete_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = Hostname::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host::DELETE,
        actions::resource_kinds::HOST,
        name.as_str(),
    )
    .await?;
    remove_host_contacts(&state, &name).await?;
    state.services.hosts().delete(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn remove_host_contacts(state: &AppState, host: &Hostname) -> Result<(), AppError> {
    let contacts = state
        .services
        .host_contacts()
        .list(
            &PageRequest::all(),
            &crate::domain::filters::HostContactFilter::default(),
        )
        .await?
        .items;
    for contact in contacts {
        if contact.hosts().iter().any(|candidate| candidate == host) {
            state
                .services
                .host_contacts()
                .delete(contact.email())
                .await?;
            let remaining = contact
                .hosts()
                .iter()
                .filter(|candidate| *candidate != host)
                .cloned()
                .collect::<Vec<_>>();
            if !remaining.is_empty() {
                state
                    .services
                    .host_contacts()
                    .create(CreateHostContact::new(
                        contact.email().clone(),
                        contact.display_name().map(str::to_string),
                        remaining,
                    ))
                    .await?;
            }
        }
    }
    Ok(())
}

#[derive(Deserialize, Default)]
struct LegacyUpdateHost {
    name: Option<String>,
    comment: Option<String>,
    contacts: Option<Vec<String>>,
    #[serde(default)]
    ttl: UpdateField<u32>,
}

async fn update_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyUpdateHost>,
) -> Result<HttpResponse, AppError> {
    let current_name = Hostname::new(name.into_inner())?;
    let payload = payload.into_inner();
    if let Some(contacts) = payload.contacts.as_ref() {
        let invalid = contacts
            .iter()
            .filter(|email| EmailAddressValue::new((*email).clone()).is_err())
            .cloned()
            .collect::<Vec<_>>();
        if !invalid.is_empty() {
            return Ok(HttpResponse::BadRequest().json(json!({
                "type": "validation_error",
                "errors": [{
                    "code": "invalid",
                    "detail": format!("Invalid email address(es): {}", invalid.join(", ")),
                    "attr": "contacts",
                }],
            })));
        }
    }
    let new_name = payload.name.map(Hostname::new).transpose()?;
    let contacts_to_migrate = if new_name.is_some() && payload.contacts.is_none() {
        state
            .services
            .host_contacts()
            .list(
                &PageRequest::all(),
                &crate::domain::filters::HostContactFilter::default(),
            )
            .await?
            .items
            .into_iter()
            .filter(|contact| contact.hosts().iter().any(|host| host == &current_name))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let ttl = match payload.ttl {
        UpdateField::Unchanged => UpdateField::Unchanged,
        UpdateField::Clear | UpdateField::Set(0) => UpdateField::Clear,
        UpdateField::Set(value) => UpdateField::Set(Ttl::new(value)?),
    };
    for action in [
        payload
            .comment
            .as_ref()
            .map(|_| actions::host::UPDATE_COMMENT),
        payload
            .ttl
            .is_changed()
            .then_some(actions::host::UPDATE_TTL),
        new_name.as_ref().map(|_| actions::host::UPDATE_NAME),
    ]
    .into_iter()
    .flatten()
    {
        authorize(
            &req,
            &state,
            action,
            actions::resource_kinds::HOST,
            current_name.as_str(),
        )
        .await?;
    }
    if let Some(contacts) = payload.contacts {
        remove_host_contacts(&state, &current_name).await?;
        for email in contacts {
            state
                .services
                .host_contacts()
                .create(CreateHostContact::new(
                    EmailAddressValue::new(email)?,
                    None,
                    vec![current_name.clone()],
                ))
                .await?;
        }
    }
    state
        .services
        .hosts()
        .update(
            &current_name,
            UpdateHost {
                name: new_name.clone(),
                ttl,
                comment: payload.comment,
                zone: UpdateField::Unchanged,
            },
        )
        .await?;
    if let Some(new_name) = new_name {
        for contact in contacts_to_migrate {
            state
                .services
                .host_contacts()
                .delete(contact.email())
                .await?;
            let hosts = contact
                .hosts()
                .iter()
                .map(|host| {
                    if host == &current_name {
                        new_name.clone()
                    } else {
                        host.clone()
                    }
                })
                .collect();
            state
                .services
                .host_contacts()
                .create(CreateHostContact::new(
                    contact.email().clone(),
                    contact.display_name().map(str::to_string),
                    hosts,
                ))
                .await?;
        }
    }
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum LegacyU32 {
    Number(u32),
    String(String),
}

impl LegacyU32 {
    fn parse(self, field: &str) -> Result<u32, AppError> {
        match self {
            Self::Number(value) => Ok(value),
            Self::String(value) => value
                .parse()
                .map_err(|_| AppError::validation(format!("{field} must be an integer"))),
        }
    }
}

#[derive(Deserialize)]
struct LegacyCreateNetwork {
    network: String,
    description: String,
    vlan: Option<LegacyU32>,
    #[serde(default)]
    dns_delegated: bool,
    #[serde(default)]
    category: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    frozen: bool,
    #[serde(default = "legacy_default_reserved")]
    reserved: u32,
}

const fn legacy_default_reserved() -> u32 {
    3
}

async fn create_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateNetwork>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    authorize(
        &req,
        &state,
        actions::network::CREATE,
        actions::resource_kinds::NETWORK,
        &payload.network,
    )
    .await?;
    let cidr = CidrValue::new(payload.network)?;
    let command = CreateNetwork::new_full(
        cidr.clone(),
        payload.description,
        payload
            .vlan
            .map(|value| value.parse("vlan").and_then(VlanId::new))
            .transpose()?,
        payload.dns_delegated,
        payload.category,
        payload.location,
        payload.frozen,
        ReservedCount::new(payload.reserved.saturating_add(1))?,
    )?;
    state.services.networks().create(command).await?;
    Ok(HttpResponse::Created()
        .append_header(("Location", format!("/api/v1/networks/{}", cidr.as_str())))
        .finish())
}

#[derive(Deserialize, Default)]
struct LegacyUpdateNetwork {
    description: Option<String>,
    #[serde(default)]
    vlan: UpdateField<LegacyU32>,
    dns_delegated: Option<bool>,
    category: Option<String>,
    location: Option<String>,
    frozen: Option<bool>,
    reserved: Option<u32>,
}

async fn update_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
    payload: web::Json<LegacyUpdateNetwork>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(network.into_inner())?;
    let payload = payload.into_inner();
    let mut update_actions = Vec::new();
    if payload.description.is_some() {
        update_actions.push(actions::network::UPDATE_DESCRIPTION);
    }
    if payload.vlan.is_changed() {
        update_actions.push(actions::network::UPDATE_VLAN);
    }
    if payload.dns_delegated.is_some() {
        update_actions.push(actions::network::UPDATE_DNS_DELEGATED);
    }
    if payload.category.is_some() {
        update_actions.push(actions::network::UPDATE_CATEGORY);
    }
    if payload.location.is_some() {
        update_actions.push(actions::network::UPDATE_LOCATION);
    }
    if payload.frozen.is_some() {
        update_actions.push(actions::network::UPDATE_FROZEN);
    }
    if payload.reserved.is_some() {
        update_actions.push(actions::network::UPDATE_RESERVED);
    }
    for action in update_actions {
        authorize(
            &req,
            &state,
            action,
            actions::resource_kinds::NETWORK,
            &cidr.as_str(),
        )
        .await?;
    }
    state
        .services
        .networks()
        .update(
            &cidr,
            UpdateNetwork {
                description: payload.description,
                vlan: payload
                    .vlan
                    .try_map(|value| value.parse("vlan").and_then(VlanId::new))?,
                dns_delegated: payload.dns_delegated,
                category: payload.category,
                location: payload.location,
                frozen: payload.frozen,
                reserved: payload
                    .reserved
                    .map(|value| ReservedCount::new(value.saturating_add(1)))
                    .transpose()?,
            },
        )
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(network.into_inner())?;
    authorize(
        &req,
        &state,
        actions::network::DELETE,
        actions::resource_kinds::NETWORK,
        &cidr.as_str(),
    )
    .await?;
    state.services.networks().delete(&cidr).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
struct LegacyCreateForwardZone {
    name: String,
    email: String,
    primary_ns: Vec<String>,
}

async fn create_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateForwardZone>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    authorize(
        &req,
        &state,
        actions::zone::forward::CREATE,
        actions::resource_kinds::FORWARD_ZONE,
        &payload.name,
    )
    .await?;
    let nameservers = payload
        .primary_ns
        .into_iter()
        .map(DnsName::new)
        .collect::<Result<Vec<_>, _>>()?;
    let primary_ns = nameservers
        .first()
        .cloned()
        .ok_or_else(|| AppError::validation("primary_ns must contain at least one name"))?;
    ensure_nameservers(&state, &nameservers).await?;
    let name = ZoneName::new(payload.name)?;
    let command = CreateForwardZone::new(
        name.clone(),
        primary_ns,
        nameservers,
        EmailAddressValue::new(payload.email)?,
        SerialNumber::new(1)?,
        SoaSeconds::new(10_800)?,
        SoaSeconds::new(3_600)?,
        SoaSeconds::new(1_814_400)?,
        Ttl::new(43_200)?,
        Ttl::new(43_200)?,
    );
    state.services.zones().create_forward(command).await?;
    // DRF's Location header makes mreg-api fetch and cache the created object.
    Ok(HttpResponse::Created()
        .append_header((
            "Location",
            format!("/api/v1/zones/forward/{}", name.as_str()),
        ))
        .finish())
}

async fn ensure_nameservers(state: &AppState, names: &[DnsName]) -> Result<(), AppError> {
    for name in names {
        if state.services.nameservers().get(name).await.is_err() {
            state
                .services
                .nameservers()
                .create(CreateNameServer::new(name.clone(), None))
                .await?;
        }
    }
    Ok(())
}

#[derive(Deserialize, Default)]
struct LegacyUpdateForwardZone {
    email: Option<String>,
    #[serde(rename = "serialno")]
    serial_no: Option<u64>,
    refresh: Option<u32>,
    retry: Option<u32>,
    expire: Option<u32>,
    soa_ttl: Option<u32>,
    default_ttl: Option<u32>,
}

async fn update_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyUpdateForwardZone>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(name.into_inner())?;
    let payload = payload.into_inner();
    authorize(
        &req,
        &state,
        actions::zone::forward::UPDATE_TIMING,
        actions::resource_kinds::FORWARD_ZONE,
        name.as_str(),
    )
    .await?;
    state
        .services
        .zones()
        .update_forward(
            &name,
            UpdateForwardZone {
                primary_ns: None,
                nameservers: None,
                email: payload.email.map(EmailAddressValue::new).transpose()?,
                serial_no: payload.serial_no.map(SerialNumber::new).transpose()?,
                refresh: payload.refresh.map(SoaSeconds::new).transpose()?,
                retry: payload.retry.map(SoaSeconds::new).transpose()?,
                expire: payload.expire.map(SoaSeconds::new).transpose()?,
                soa_ttl: payload.soa_ttl.map(Ttl::new).transpose()?,
                default_ttl: payload.default_ttl.map(Ttl::new).transpose()?,
            },
        )
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
struct LegacyUpdateNameservers {
    primary_ns: Vec<String>,
}

async fn update_forward_zone_nameservers(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyUpdateNameservers>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(name.into_inner())?;
    let nameservers = payload
        .into_inner()
        .primary_ns
        .into_iter()
        .map(DnsName::new)
        .collect::<Result<Vec<_>, _>>()?;
    let primary_ns = nameservers
        .first()
        .cloned()
        .ok_or_else(|| AppError::validation("primary_ns must contain at least one name"))?;
    ensure_nameservers(&state, &nameservers).await?;
    authorize(
        &req,
        &state,
        actions::zone::forward::UPDATE_NAMESERVERS,
        actions::resource_kinds::FORWARD_ZONE,
        name.as_str(),
    )
    .await?;
    state
        .services
        .zones()
        .update_forward(
            &name,
            UpdateForwardZone {
                primary_ns: Some(primary_ns),
                nameservers: Some(nameservers),
                ..UpdateForwardZone::default()
            },
        )
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::zone::forward::DELETE,
        actions::resource_kinds::FORWARD_ZONE,
        name.as_str(),
    )
    .await?;
    state.services.zones().delete_forward(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

fn paginate(values: Vec<Value>, query: LegacyPageQuery) -> Value {
    let count = values.len();
    let page = query.page.max(1);
    let size = query.page_size.clamp(1, 1000);
    let start = page.saturating_sub(1).saturating_mul(size).min(count);
    let end = start.saturating_add(size).min(count);
    json!({
        "count": count,
        "next": (end < count).then(|| format!("?page={}&page_size={size}", page + 1)),
        "previous": (page > 1).then(|| format!("?page={}&page_size={size}", page - 1)),
        "results": values[start..end].to_vec(),
    })
}

fn label_json(value: &crate::domain::label::Label) -> Value {
    json!({"id": legacy_id(value.id()), "name": value.name().as_str(), "description": value.description(), "created_at": value.created_at(), "updated_at": value.updated_at()})
}

async fn labels(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::label::LIST,
        actions::resource_kinds::LABEL,
        "*",
    )
    .await?;
    let labels = state
        .services
        .labels()
        .list(&PageRequest::all())
        .await?
        .items;
    let values = labels
        .iter()
        .filter(|label| query.id.is_none_or(|id| legacy_id(label.id()) == id))
        .filter(|label| {
            query
                .name
                .as_ref()
                .is_none_or(|name| label.name().as_str() == name)
        })
        .map(label_json)
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

#[derive(Deserialize)]
struct LegacyLabelPayload {
    name: Option<String>,
    description: Option<String>,
}

async fn create_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyLabelPayload>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = LabelName::new(
        payload
            .name
            .ok_or_else(|| AppError::validation("name is required"))?,
    )?;
    authorize(
        &req,
        &state,
        actions::label::CREATE,
        actions::resource_kinds::LABEL,
        name.as_str(),
    )
    .await?;
    state
        .services
        .labels()
        .create(CreateLabel::new(
            name,
            payload
                .description
                .ok_or_else(|| AppError::validation("description is required"))?,
        )?)
        .await?;
    Ok(HttpResponse::Created().finish())
}

async fn find_label_by_legacy_id(
    state: &AppState,
    id: u32,
) -> Result<crate::domain::label::Label, AppError> {
    state
        .services
        .labels()
        .list(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|label| legacy_id(label.id()) == id)
        .ok_or_else(|| AppError::not_found("label was not found"))
}

async fn label_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let label = find_label_by_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::label::GET,
        actions::resource_kinds::LABEL,
        label.name().as_str(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(label_json(&label)))
}

async fn update_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
    payload: web::Json<LegacyLabelPayload>,
) -> Result<HttpResponse, AppError> {
    let label = find_label_by_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::label::UPDATE_DESCRIPTION,
        actions::resource_kinds::LABEL,
        label.name().as_str(),
    )
    .await?;
    let payload = payload.into_inner();
    let command = UpdateLabel::new(payload.description)?
        .with_name(payload.name.map(LabelName::new).transpose()?);
    state
        .services
        .labels()
        .update(label.name(), command)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let label = find_label_by_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::label::DELETE,
        actions::resource_kinds::LABEL,
        label.name().as_str(),
    )
    .await?;
    state.services.labels().delete(label.name()).await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn label_by_name(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = LabelName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::label::GET,
        actions::resource_kinds::LABEL,
        name.as_str(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(label_json(&state.services.labels().get(&name).await?)))
}

fn nameserver_json(value: &crate::domain::nameserver::NameServer) -> Value {
    json!({"id": legacy_id(value.id()), "name": value.name().as_str(), "ttl": value.ttl().map(|ttl| ttl.as_u32()), "created_at": value.created_at(), "updated_at": value.updated_at()})
}

async fn nameservers(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::nameserver::LIST,
        actions::resource_kinds::NAMESERVER,
        "*",
    )
    .await?;
    let values = state
        .services
        .nameservers()
        .list(&PageRequest::all())
        .await?
        .items
        .iter()
        .map(nameserver_json)
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

async fn nameserver_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = DnsName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::nameserver::GET,
        actions::resource_kinds::NAMESERVER,
        name.as_str(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(nameserver_json(
        &state.services.nameservers().get(&name).await?,
    )))
}

fn bacnet_json(value: &crate::domain::bacnet::BacnetIdAssignment) -> Value {
    json!({"id": value.bacnet_id().as_u32(), "host": value.host_name().as_str(), "hostname": value.host_name().as_str()})
}

async fn bacnet_ids(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::bacnet_id::LIST,
        actions::resource_kinds::BACNET_ID,
        "*",
    )
    .await?;
    let values = state
        .services
        .bacnet()
        .list(&PageRequest::all(), &BacnetIdFilter::default())
        .await?
        .items
        .iter()
        .map(bacnet_json)
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

async fn bacnet_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let id = BacnetIdentifier::new(id.into_inner())?;
    authorize(
        &req,
        &state,
        actions::bacnet_id::GET,
        actions::resource_kinds::BACNET_ID,
        &id.as_u32().to_string(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(bacnet_json(&state.services.bacnet().get(id).await?)))
}

async fn host_related(
    state: &web::Data<AppState>,
) -> Result<
    (
        Vec<crate::domain::host::IpAddressAssignment>,
        Vec<RecordInstance>,
        Vec<crate::domain::host_group::HostGroup>,
        Vec<crate::domain::host_contact::HostContact>,
        Vec<HostPolicyRole>,
        Vec<crate::domain::ptr_override::PtrOverride>,
    ),
    AppError,
> {
    let ips = state
        .services
        .hosts()
        .list_ip_addresses(&PageRequest::all())
        .await?
        .items;
    let records = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items;
    let groups = state
        .services
        .host_groups()
        .list(&PageRequest::all(), &HostGroupFilter::default())
        .await?
        .items;
    let contacts = state
        .services
        .host_contacts()
        .list(
            &PageRequest::all(),
            &crate::domain::filters::HostContactFilter::default(),
        )
        .await?
        .items;
    let roles = policy_roles(state).await?;
    let ptr_overrides = state
        .services
        .ptr_overrides()
        .list(&PageRequest::all(), &PtrOverrideFilter::default())
        .await?
        .items;
    Ok((ips, records, groups, contacts, roles, ptr_overrides))
}

fn ip_json(value: &crate::domain::host::IpAddressAssignment, host_id: Option<u32>) -> Value {
    json!({"id": legacy_ip_id(value), "host": host_id, "ipaddress": value.address().as_str(), "macaddress": value.mac_address().map(|mac| mac.as_str()).unwrap_or_default(), "created_at": value.created_at(), "updated_at": value.updated_at()})
}

fn legacy_ip_id(value: &crate::domain::host::IpAddressAssignment) -> u32 {
    legacy_id(value.id())
}

fn record_json(value: &RecordInstance, host_id: Option<u32>) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("id".into(), json!(legacy_id(value.id())));
    object.insert("host".into(), json!(host_id));
    let data = value.data();
    match value.type_name().as_str() {
        "CNAME" => {
            object.insert("name".into(), json!(value.owner_name()));
            object.insert("ttl".into(), json!(value.ttl().map(|ttl| ttl.as_u32())));
            object.insert("zone".into(), json!(value.zone_id().map(legacy_id)));
        }
        "MX" => {
            object.insert("priority".into(), data["preference"].clone());
            object.insert("mx".into(), data["exchange"].clone());
        }
        "NAPTR" => {
            object.insert("order".into(), data["order"].clone());
            object.insert("preference".into(), data["preference"].clone());
            object.insert("flag".into(), data["flags"].clone());
            object.insert("service".into(), data["services"].clone());
            object.insert("regex".into(), data["regexp"].clone());
            object.insert("replacement".into(), data["replacement"].clone());
        }
        "LOC" => {
            object.insert("loc".into(), json!(legacy_loc_string(data)));
        }
        "SSHFP" => {
            object.insert("algorithm".into(), data["algorithm"].clone());
            object.insert("hash_type".into(), data["fp_type"].clone());
            let fingerprint = data["fingerprint"].as_str().unwrap_or_default();
            let fingerprint = fingerprint
                .strip_prefix("f1c0")
                .and_then(|encoded| {
                    let length = usize::from_str_radix(encoded.get(..4)?, 16).ok()?;
                    encoded.get(4..4 + length)
                })
                .unwrap_or(fingerprint);
            object.insert("fingerprint".into(), json!(fingerprint));
            object.insert("ttl".into(), json!(value.ttl().map(|ttl| ttl.as_u32())));
        }
        "SRV" => {
            object.insert("name".into(), json!(value.owner_name()));
            object.insert("priority".into(), data["priority"].clone());
            object.insert("weight".into(), data["weight"].clone());
            object.insert("port".into(), data["port"].clone());
            object.insert("ttl".into(), json!(value.ttl().map(|ttl| ttl.as_u32())));
            object.insert("zone".into(), json!(value.zone_id().map(legacy_id)));
        }
        "TXT" => {
            let txt = data
                .get("value")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("")
                })
                .or_else(|| {
                    data.get("value")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            object.insert("txt".into(), json!(txt));
        }
        _ => {
            if let Some(data) = data.as_object() {
                for (key, item) in data {
                    object.insert(key.clone(), item.clone());
                }
            }
        }
    }
    object.insert("created_at".into(), json!(value.created_at()));
    object.insert("updated_at".into(), json!(value.updated_at()));
    Value::Object(object)
}

fn host_json(
    value: &crate::domain::host::Host,
    ips: &[crate::domain::host::IpAddressAssignment],
    records: &[RecordInstance],
    groups: &[crate::domain::host_group::HostGroup],
    contacts: &[crate::domain::host_contact::HostContact],
    roles: &[HostPolicyRole],
    ptr_overrides: &[crate::domain::ptr_override::PtrOverride],
) -> Value {
    let id = legacy_id(value.id());
    let host_records = records
        .iter()
        .filter(|record| {
            record.owner_id() == Some(value.id()) || record.owner_name() == value.name().as_str()
        })
        .collect::<Vec<_>>();
    let records_of = |kind: &str| {
        let mut values = host_records
            .iter()
            .filter(|record| record.type_name().as_str() == kind)
            .map(|record| {
                let mut value = record_json(record, Some(id));
                if kind == "NAPTR"
                    && let Some(service) = value["service"].as_str()
                {
                    value["service"] = json!(service.to_ascii_lowercase());
                }
                value
            })
            .collect::<Vec<_>>();
        if kind == "NAPTR" {
            values.sort_by(|left, right| left["service"].as_str().cmp(&right["service"].as_str()));
        }
        values
    };
    let contacts = contacts
        .iter()
        .filter(|contact| contact.hosts().iter().any(|host| host == value.name()))
        .map(|contact| {
            json!({
                "id": legacy_id(contact.id()), "email": contact.email().as_str(),
                "created_at": contact.created_at(), "updated_at": contact.updated_at(),
            })
        })
        .collect::<Vec<_>>();
    let contact = contacts
        .first()
        .and_then(|value| value["email"].as_str())
        .unwrap_or("");
    let mut host_ips = ips
        .iter()
        .filter(|ip| ip.host_id() == value.id())
        .map(|ip| ip_json(ip, Some(id)))
        .collect::<Vec<_>>();
    host_ips.sort_by(|left, right| left["ipaddress"].as_str().cmp(&right["ipaddress"].as_str()));
    let host_ptr_overrides = ptr_overrides
        .iter()
        .filter(|ptr| ptr.host_name() == value.name())
        .map(|ptr| ptr_override_json(ptr, Some(id)))
        .collect::<Vec<_>>();
    json!({
        "id": id, "name": value.name().as_str(), "zone": value.zone().map(|_| 1),
        "ttl": value.ttl().map(|ttl| ttl.as_u32()), "comment": value.comment(),
        "ipaddresses": host_ips,
        "cnames": records_of("CNAME"), "mxs": records_of("MX"), "txts": records_of("TXT"),
        "srvs": records_of("SRV"), "naptrs": records_of("NAPTR"), "sshfps": records_of("SSHFP"),
        "hinfo": records_of("HINFO").into_iter().next(), "loc": records_of("LOC").into_iter().next(),
        "hostgroups": groups.iter().filter(|group| group.hosts().iter().any(|host| host == value.name())).map(|group| group.name().as_str()).collect::<Vec<_>>(),
        "ptr_overrides": host_ptr_overrides,
        "roles": roles.iter().filter(|role| role.hosts().iter().any(|host| host == value.name().as_str())).map(|role| role.name().as_str()).collect::<Vec<_>>(),
        "bacnetid": null, "communities": [], "contacts": contacts, "contact": contact,
        "created_at": value.created_at(), "updated_at": value.updated_at(),
    })
}

async fn hosts(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::host::LIST,
        actions::resource_kinds::HOST,
        "*",
    )
    .await?;
    let page = state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?;
    let (ips, records, groups, contacts, roles, ptr_overrides) = host_related(&state).await?;
    let requested_ids = query.ids.as_deref().map(|ids| {
        ids.split(',')
            .filter_map(|value| value.parse::<u32>().ok())
            .collect::<Vec<_>>()
    });
    let regex_contains = |value: &str, pattern: &str| {
        let needle = pattern.trim_matches(|character| matches!(character, '.' | '*' | '^' | '$'));
        value.contains(needle)
    };
    let values = page
        .items
        .iter()
        .filter(|host| query.id.is_none_or(|id| legacy_id(host.id()) == id))
        .map(|host| {
            host_json(
                host,
                &ips,
                &records,
                &groups,
                &contacts,
                &roles,
                &ptr_overrides,
            )
        })
        .filter(|host| {
            requested_ids.as_ref().is_none_or(|ids| {
                host["id"]
                    .as_u64()
                    .is_some_and(|id| ids.contains(&(id as u32)))
            }) && query.host_ipaddress.as_deref().is_none_or(|address| {
                host["ipaddresses"].as_array().is_some_and(|ips| {
                    ips.iter()
                        .any(|ip| ip["ipaddress"].as_str() == Some(address))
                })
            }) && query.host_ptr_address.as_deref().is_none_or(|address| {
                host["ptr_overrides"].as_array().is_some_and(|ptrs| {
                    ptrs.iter()
                        .any(|ptr| ptr["ipaddress"].as_str() == Some(address))
                })
            }) && query
                .zone
                .is_none_or(|zone| host["zone"].as_u64() == Some(zone as u64))
                && query.name_regex.as_deref().is_none_or(|pattern| {
                    host["name"]
                        .as_str()
                        .is_some_and(|name| regex_contains(name, pattern))
                })
                && query.comment_regex.as_deref().is_none_or(|pattern| {
                    host["comment"]
                        .as_str()
                        .is_some_and(|comment| regex_contains(comment, pattern))
                })
                && query.contact_regex.as_deref().is_none_or(|pattern| {
                    host["contact"]
                        .as_str()
                        .is_some_and(|contact| regex_contains(contact, pattern))
                })
        })
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

async fn host_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = Hostname::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host::GET,
        actions::resource_kinds::HOST,
        name.as_str(),
    )
    .await?;
    let host = state.services.hosts().get(&name).await?;
    let (ips, records, groups, contacts, roles, ptr_overrides) = host_related(&state).await?;
    Ok(HttpResponse::Ok().json(host_json(
        &host,
        &ips,
        &records,
        &groups,
        &contacts,
        &roles,
        &ptr_overrides,
    )))
}

async fn ip_addresses(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::host::ip::LIST,
        actions::resource_kinds::IP_ADDRESS,
        "*",
    )
    .await?;
    let names = state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?
        .items
        .into_iter()
        .map(|host| (host.id(), legacy_id(host.id())))
        .collect::<HashMap<_, _>>();
    let mut assignments = state
        .services
        .hosts()
        .list_ip_addresses(&PageRequest::all())
        .await?
        .items;
    assignments.retain(|ip| {
        query.host.is_none_or(|id| legacy_id(ip.host_id()) == id)
            && query
                .ipaddress
                .as_deref()
                .is_none_or(|address| ip.address().as_str() == address)
            && query.macaddress.as_ref().is_none_or(|mac| {
                ip.mac_address()
                    .is_some_and(|value| value.as_str().eq_ignore_ascii_case(mac))
            })
    });
    let values = assignments
        .iter()
        .map(|ip| ip_json(ip, names.get(&ip.host_id()).copied()))
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

fn host_group_json(
    value: &crate::domain::host_group::HostGroup,
    groups: &[crate::domain::host_group::HostGroup],
) -> Value {
    let mut children = groups
        .iter()
        .filter(|group| {
            group
                .parent_groups()
                .iter()
                .any(|parent| parent == value.name())
        })
        .map(|group| json!({"name": group.name().as_str()}))
        .collect::<Vec<_>>();
    children.sort_by_key(|item| item["name"].as_str().unwrap_or_default().to_string());
    let mut parents = value
        .parent_groups()
        .iter()
        .map(|name| json!({"name": name.as_str()}))
        .collect::<Vec<_>>();
    let mut hosts = value
        .hosts()
        .iter()
        .map(|name| json!({"name": name.as_str()}))
        .collect::<Vec<_>>();
    let owners = value
        .owner_groups()
        .iter()
        .map(|name| json!({"name": name.as_str()}))
        .collect::<Vec<_>>();
    for items in [&mut parents, &mut hosts] {
        items.sort_by_key(|item| item["name"].as_str().unwrap_or_default().to_string());
    }
    json!({"id": legacy_name_id(value.name().as_str()), "name": value.name().as_str(), "description": value.description(), "parent": parents, "groups": children, "hosts": hosts, "owners": owners, "created_at": value.created_at(), "updated_at": value.updated_at()})
}

async fn host_groups(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::host_group::LIST,
        actions::resource_kinds::HOST_GROUP,
        "*",
    )
    .await?;
    let groups = state
        .services
        .host_groups()
        .list(&PageRequest::all(), &HostGroupFilter::default())
        .await?
        .items;
    let values = groups
        .iter()
        .filter(|group| {
            query
                .id
                .is_none_or(|id| legacy_name_id(group.name().as_str()) == id)
                && query.name_regex.as_ref().is_none_or(|pattern| {
                    let needle = pattern
                        .trim_matches(|character| matches!(character, '.' | '*' | '^' | '$'));
                    group.name().as_str().contains(needle)
                })
        })
        .map(|group| host_group_json(group, &groups))
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
async fn host_group_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostGroupName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_group::GET,
        actions::resource_kinds::HOST_GROUP,
        name.as_str(),
    )
    .await?;
    let group = state.services.host_groups().get(&name).await?;
    let groups = state
        .services
        .host_groups()
        .list(&PageRequest::all(), &HostGroupFilter::default())
        .await?
        .items;
    let mut response = host_group_json(&group, &groups);
    if let Some(owners) = response["owners"].as_array_mut() {
        owners.sort_by_key(|item| item["name"].as_str().unwrap_or_default().to_string());
    }
    Ok(HttpResponse::Ok().json(response))
}

async fn ptr_overrides(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::ptr_override::LIST,
        actions::resource_kinds::PTR_OVERRIDE,
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
        .map(|host| (host.name().as_str().to_string(), legacy_id(host.id())))
        .collect::<HashMap<_, _>>();
    let values = state
        .services
        .ptr_overrides()
        .list(&PageRequest::all(), &PtrOverrideFilter::default())
        .await?
        .items
        .iter()
        .filter(|value| {
            query
                .id
                .is_none_or(|id| legacy_name_id(&value.address().as_str()) == id)
                && query
                    .ipaddress
                    .as_deref()
                    .is_none_or(|address| value.address().as_str() == address)
        })
        .map(|value| ptr_override_json(value, hosts.get(value.host_name().as_str()).copied()))
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

fn ptr_override_json(value: &crate::domain::ptr_override::PtrOverride, host: Option<u32>) -> Value {
    json!({"id": legacy_name_id(&value.address().as_str()), "host": host, "ipaddress": value.address().as_str(), "created_at": value.created_at(), "updated_at": value.updated_at()})
}

#[derive(Deserialize)]
struct LegacyPtrOverridePayload {
    host: u32,
    ipaddress: Option<String>,
}

async fn host_from_legacy_id(
    state: &AppState,
    id: u32,
) -> Result<crate::domain::host::Host, AppError> {
    state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?
        .items
        .into_iter()
        .find(|host| legacy_id(host.id()) == id)
        .ok_or_else(|| AppError::not_found("host was not found"))
}

async fn create_ptr_override(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyPtrOverridePayload>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let host = host_from_legacy_id(&state, payload.host).await?;
    let address = IpAddressValue::new(
        payload
            .ipaddress
            .ok_or_else(|| AppError::validation("ipaddress is required"))?,
    )?;
    authorize(
        &req,
        &state,
        actions::ptr_override::CREATE,
        actions::resource_kinds::PTR_OVERRIDE,
        &address.as_str(),
    )
    .await?;
    let value = state
        .services
        .ptr_overrides()
        .create(CreatePtrOverride::new(host.name().clone(), address, None))
        .await?;
    Ok(HttpResponse::Created().json(ptr_override_json(&value, Some(legacy_id(host.id())))))
}

async fn find_ptr_override_by_legacy_id(
    state: &AppState,
    id: u32,
) -> Result<crate::domain::ptr_override::PtrOverride, AppError> {
    state
        .services
        .ptr_overrides()
        .list(&PageRequest::all(), &PtrOverrideFilter::default())
        .await?
        .items
        .into_iter()
        .find(|value| legacy_name_id(&value.address().as_str()) == id)
        .ok_or_else(|| AppError::not_found("PTR override was not found"))
}

async fn update_ptr_override(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
    payload: web::Json<LegacyPtrOverridePayload>,
) -> Result<HttpResponse, AppError> {
    let old = find_ptr_override_by_legacy_id(&state, id.into_inner()).await?;
    let host = host_from_legacy_id(&state, payload.host).await?;
    authorize(
        &req,
        &state,
        actions::ptr_override::DELETE,
        actions::resource_kinds::PTR_OVERRIDE,
        &old.address().as_str(),
    )
    .await?;
    state.services.ptr_overrides().delete(old.address()).await?;
    state
        .services
        .ptr_overrides()
        .create(CreatePtrOverride::new(
            host.name().clone(),
            *old.address(),
            old.target_name().cloned(),
        ))
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_ptr_override(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let value = find_ptr_override_by_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::ptr_override::DELETE,
        actions::resource_kinds::PTR_OVERRIDE,
        &value.address().as_str(),
    )
    .await?;
    state
        .services
        .ptr_overrides()
        .delete(value.address())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn networks(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::network::LIST,
        actions::resource_kinds::NETWORK,
        "*",
    )
    .await?;
    let mut networks = state
        .services
        .networks()
        .list(&PageRequest::all(), &NetworkFilter::default())
        .await?
        .items;
    networks.retain(|network| {
        query.id.is_none_or(|id| legacy_id(network.id()) == id)
            && query
                .network
                .as_ref()
                .is_none_or(|value| network.cidr().as_str() == *value)
            && query.description_regex.as_ref().is_none_or(|pattern| {
                let needle = pattern.trim_matches('.').trim_matches('*');
                network.description().contains(needle)
            })
            && query
                .vlan
                .is_none_or(|value| network.vlan().map(|vlan| vlan.as_u32()) == Some(value))
            && query
                .dns_delegated
                .is_none_or(|value| network.dns_delegated() == (value != 0))
            && query
                .category
                .as_ref()
                .is_none_or(|value| network.category() == value)
            && query
                .location
                .as_ref()
                .is_none_or(|value| network.location() == value)
            && query
                .frozen
                .is_none_or(|value| network.frozen() == (value != 0))
            && query
                .reserved
                .is_none_or(|value| network.reserved().as_u32().saturating_sub(1) == value)
    });
    let mut values = Vec::with_capacity(networks.len());
    for network in &networks {
        values.push(super::reads::network_json(&state, network).await?);
    }
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
async fn network_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let cidr = crate::domain::types::CidrValue::new(network.into_inner())?;
    authorize(
        &req,
        &state,
        actions::network::GET,
        actions::resource_kinds::NETWORK,
        &cidr.as_str(),
    )
    .await?;
    let network = state.services.networks().get(&cidr).await?;
    Ok(HttpResponse::Ok().json(super::reads::network_json(&state, &network).await?))
}

async fn forward_zones(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::zone::forward::LIST,
        actions::resource_kinds::FORWARD_ZONE,
        "*",
    )
    .await?;
    let zones = state
        .services
        .zones()
        .list_forward(&PageRequest::all())
        .await?
        .items;
    let mut values = Vec::with_capacity(zones.len());
    for zone in &zones {
        values.push(super::reads::forward_zone_json(&state, zone).await);
    }
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
async fn forward_zone_detail(
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
    Ok(HttpResponse::Ok().json(super::reads::forward_zone_json(&state, &zone).await))
}

fn reverse_zone_json(zone: &crate::domain::zone::ReverseZone) -> Value {
    json!({"id": zone.id(), "name": zone.name().as_str(), "network": zone.network().map(|network| network.as_str()), "updated": zone.updated(), "primary_ns": zone.primary_ns().as_str(), "nameservers": zone.nameservers().iter().map(|value| json!({"name": value.as_str()})).collect::<Vec<_>>(), "email": zone.email().as_str(), "serialno": zone.serial_no().as_u64(), "serialno_updated_at": zone.serial_no_updated_at(), "refresh": zone.refresh().as_u32(), "retry": zone.retry().as_u32(), "expire": zone.expire().as_u32(), "soa_ttl": zone.soa_ttl().as_u32(), "default_ttl": zone.default_ttl().as_u32(), "created_at": zone.created_at(), "updated_at": zone.updated_at()})
}
async fn reverse_zones(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::zone::reverse::LIST,
        actions::resource_kinds::REVERSE_ZONE,
        "*",
    )
    .await?;
    let values = state
        .services
        .zones()
        .list_reverse(&PageRequest::all())
        .await?
        .items
        .iter()
        .map(reverse_zone_json)
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
async fn reverse_zone_detail(
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
    Ok(HttpResponse::Ok().json(reverse_zone_json(
        &state.services.zones().get_reverse(&name).await?,
    )))
}

fn policy_json(value: &NetworkPolicyDetails) -> Value {
    let policy = value.policy();
    json!({
        "id": legacy_id(policy.id()),
        "name": policy.name().as_str(),
        "description": policy.description(),
        "attributes": value.attributes().iter().map(|attribute| json!({
            "name": attribute.name().as_str(), "value": attribute.value()
        })).collect::<Vec<_>>(),
        "community_template_pattern": policy.community_template_pattern(),
        "created_at": policy.created_at(),
        "updated_at": policy.updated_at()
    })
}

async fn policy_details(
    state: &AppState,
    policy: NetworkPolicy,
) -> Result<NetworkPolicyDetails, AppError> {
    state
        .services
        .network_policies()
        .get_details(policy.name())
        .await
}
async fn network_policies(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::network_policy::LIST,
        actions::resource_kinds::NETWORK_POLICY,
        "*",
    )
    .await?;
    let mut policies = state
        .services
        .network_policies()
        .list(&PageRequest::all(), &NetworkPolicyFilter::default())
        .await?
        .items;
    policies.sort_by_key(NetworkPolicy::created_at);
    let attribute_definitions = if query.attributes_description.is_some() {
        state
            .services
            .network_policies()
            .list_attributes(&PageRequest::all())
            .await?
            .items
    } else {
        Vec::new()
    };
    let mut values = Vec::new();
    for policy in policies.into_iter().filter(|policy| {
        query.id.is_none_or(|id| legacy_id(policy.id()) == id)
            && query
                .name
                .as_deref()
                .is_none_or(|name| policy.name().as_str().eq_ignore_ascii_case(name))
            && query.name_regex.as_deref().is_none_or(|pattern| {
                let needle =
                    pattern.trim_matches(|character| matches!(character, '.' | '*' | '^' | '$'));
                policy.name().as_str().contains(needle)
            })
    }) {
        let details = policy_details(&state, policy).await?;
        let attribute_match = query.attributes.is_none_or(|id| {
            details
                .attributes()
                .iter()
                .any(|value| legacy_id(value.attribute_id()) == id)
        }) && query.attributes_name.as_deref().is_none_or(|name| {
            details
                .attributes()
                .iter()
                .any(|value| value.name().as_str().eq_ignore_ascii_case(name))
        }) && query.attributes_description.as_deref().is_none_or(
            |description| {
                details.attributes().iter().any(|value| {
                    attribute_definitions.iter().any(|attribute| {
                        attribute.id() == value.attribute_id()
                            && attribute.description() == description
                    })
                })
            },
        );
        if attribute_match {
            values.push(policy_json(&details));
        }
    }
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

#[derive(Deserialize)]
struct LegacyPolicyAttributeValue {
    name: String,
    #[serde(default)]
    value: bool,
}

impl LegacyPolicyAttributeValue {
    fn into_domain(self) -> Result<SetNetworkPolicyAttributeValue, AppError> {
        Ok(SetNetworkPolicyAttributeValue::new(
            NetworkPolicyAttributeName::new(self.name)?,
            self.value,
        ))
    }
}

#[derive(Deserialize)]
struct LegacyCreateNetworkPolicy {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    attributes: Vec<LegacyPolicyAttributeValue>,
    community_template_pattern: Option<String>,
}

async fn create_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateNetworkPolicy>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = NetworkPolicyName::new(payload.name)?;
    authorize(
        &req,
        &state,
        actions::network_policy::CREATE,
        actions::resource_kinds::NETWORK_POLICY,
        name.as_str(),
    )
    .await?;
    let policy = state
        .services
        .network_policies()
        .create(
            CreateNetworkPolicy::new(
                name,
                payload.description,
                payload.community_template_pattern,
            )?
            .with_attributes(
                payload
                    .attributes
                    .into_iter()
                    .map(LegacyPolicyAttributeValue::into_domain)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )
        .await?;
    let details = policy_details(&state, policy).await?;
    Ok(HttpResponse::Created()
        .append_header((
            "Location",
            format!(
                "/api/v1/networkpolicies/{}",
                legacy_id(details.policy().id())
            ),
        ))
        .json(policy_json(&details)))
}

async fn network_policy_from_legacy_id(
    state: &AppState,
    id: u32,
) -> Result<crate::domain::network_policy::NetworkPolicy, AppError> {
    state
        .services
        .network_policies()
        .list(&PageRequest::all(), &NetworkPolicyFilter::default())
        .await?
        .items
        .into_iter()
        .find(|policy| legacy_id(policy.id()) == id)
        .ok_or_else(|| AppError::not_found("network policy was not found"))
}

async fn network_policy_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let policy = network_policy_from_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::network_policy::GET,
        actions::resource_kinds::NETWORK_POLICY,
        policy.name().as_str(),
    )
    .await?;
    let details = policy_details(&state, policy).await?;
    Ok(HttpResponse::Ok().json(policy_json(&details)))
}

#[derive(Default, Deserialize)]
struct LegacyUpdateNetworkPolicy {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    community_template_pattern: UpdateField<String>,
    attributes: Option<Vec<LegacyPolicyAttributeValue>>,
}

async fn update_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
    payload: web::Json<LegacyUpdateNetworkPolicy>,
) -> Result<HttpResponse, AppError> {
    let old = network_policy_from_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::network_policy::UPDATE,
        actions::resource_kinds::NETWORK_POLICY,
        old.name().as_str(),
    )
    .await?;
    let payload = payload.into_inner();
    let command = UpdateNetworkPolicy {
        name: payload.name.map(NetworkPolicyName::new).transpose()?,
        description: payload.description,
        community_template_pattern: payload.community_template_pattern,
        attributes: payload
            .attributes
            .map(|values| {
                values
                    .into_iter()
                    .map(LegacyPolicyAttributeValue::into_domain)
                    .collect()
            })
            .transpose()?,
    };
    let updated = state
        .services
        .network_policies()
        .update(old.name(), command)
        .await?;
    let details = policy_details(&state, updated).await?;
    Ok(HttpResponse::Ok().json(policy_json(&details)))
}

async fn delete_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let policy = network_policy_from_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::network_policy::DELETE,
        actions::resource_kinds::NETWORK_POLICY,
        policy.name().as_str(),
    )
    .await?;
    state
        .services
        .network_policies()
        .delete(policy.name())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

fn policy_attribute_json(value: &NetworkPolicyAttribute) -> Value {
    json!({
        "id": legacy_id(value.id()), "name": value.name().as_str(),
        "description": value.description(), "created_at": value.created_at(),
        "updated_at": value.updated_at()
    })
}

async fn network_policy_attributes(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::network_policy::LIST,
        actions::resource_kinds::NETWORK_POLICY,
        "*",
    )
    .await?;
    let values = state
        .services
        .network_policies()
        .list_attributes(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .filter(|attribute| {
            query.id.is_none_or(|id| legacy_id(attribute.id()) == id)
                && query
                    .name
                    .as_deref()
                    .is_none_or(|name| attribute.name().as_str().eq_ignore_ascii_case(name))
                && query.name_regex.as_deref().is_none_or(|pattern| {
                    attribute
                        .name()
                        .as_str()
                        .contains(pattern.trim_matches(|c| matches!(c, '.' | '*' | '^' | '$')))
                })
                && query.description_regex.as_deref().is_none_or(|pattern| {
                    attribute
                        .description()
                        .contains(pattern.trim_matches(|c| matches!(c, '.' | '*' | '^' | '$')))
                })
        })
        .map(|attribute| policy_attribute_json(&attribute))
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

#[derive(Deserialize)]
struct LegacyCreateNetworkPolicyAttribute {
    name: String,
    #[serde(default)]
    description: String,
}

async fn create_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreateNetworkPolicyAttribute>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = NetworkPolicyAttributeName::new(payload.name)?;
    authorize(
        &req,
        &state,
        actions::network_policy::CREATE,
        actions::resource_kinds::NETWORK_POLICY,
        name.as_str(),
    )
    .await?;
    let attribute = state
        .services
        .network_policies()
        .create_attribute(CreateNetworkPolicyAttribute::new(name, payload.description))
        .await?;
    Ok(HttpResponse::Created()
        .append_header((
            "Location",
            format!(
                "/api/v1/networkpolicyattributes/{}",
                legacy_id(attribute.id())
            ),
        ))
        .json(policy_attribute_json(&attribute)))
}

async fn network_policy_attribute_from_legacy_id(
    state: &AppState,
    id: u32,
) -> Result<NetworkPolicyAttribute, AppError> {
    state
        .services
        .network_policies()
        .list_attributes(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .find(|attribute| legacy_id(attribute.id()) == id)
        .ok_or_else(|| AppError::not_found("network policy attribute was not found"))
}

async fn network_policy_attribute_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let attribute = network_policy_attribute_from_legacy_id(&state, id.into_inner()).await?;
    authorize(
        &req,
        &state,
        actions::network_policy::GET,
        actions::resource_kinds::NETWORK_POLICY,
        attribute.name().as_str(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(policy_attribute_json(&attribute)))
}

#[derive(Deserialize)]
struct LegacyUpdateNetworkPolicyAttribute {
    name: Option<String>,
    description: Option<String>,
}

fn is_protected_policy_attribute(name: &str) -> bool {
    name == "isolated"
        || std::env::var("MREG_PROTECTED_POLICY_ATTRIBUTES")
            .ok()
            .is_some_and(|value| value.split(',').map(str::trim).any(|item| item == name))
}

async fn update_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
    payload: web::Json<LegacyUpdateNetworkPolicyAttribute>,
) -> Result<HttpResponse, AppError> {
    let old = network_policy_attribute_from_legacy_id(&state, id.into_inner()).await?;
    let payload = payload.into_inner();
    if is_protected_policy_attribute(old.name().as_str())
        && payload
            .name
            .as_deref()
            .is_some_and(|name| !old.name().as_str().eq_ignore_ascii_case(name))
    {
        return Err(AppError::forbidden(format!(
            "Cannot rename protected attribute '{}'.",
            old.name()
        )));
    }
    authorize(
        &req,
        &state,
        actions::network_policy::UPDATE,
        actions::resource_kinds::NETWORK_POLICY,
        old.name().as_str(),
    )
    .await?;
    let updated = state
        .services
        .network_policies()
        .update_attribute(
            old.name(),
            UpdateNetworkPolicyAttribute {
                name: payload
                    .name
                    .map(NetworkPolicyAttributeName::new)
                    .transpose()?,
                description: payload.description,
            },
        )
        .await?;
    Ok(HttpResponse::Ok().json(policy_attribute_json(&updated)))
}

async fn delete_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let attribute = network_policy_attribute_from_legacy_id(&state, id.into_inner()).await?;
    if is_protected_policy_attribute(attribute.name().as_str()) {
        return Err(AppError::forbidden(format!(
            "Cannot delete the attribute '{}', it is protected.",
            attribute.name()
        )));
    }
    authorize(
        &req,
        &state,
        actions::network_policy::DELETE,
        actions::resource_kinds::NETWORK_POLICY,
        attribute.name().as_str(),
    )
    .await?;
    state
        .services
        .network_policies()
        .delete_attribute(attribute.name())
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

fn atom_json(
    value: &crate::domain::host_policy::HostPolicyAtom,
    roles: &[HostPolicyRole],
) -> Value {
    let mut memberships = roles
        .iter()
        .filter(|role| {
            role.atoms()
                .iter()
                .any(|name| name == value.name().as_str())
        })
        .map(|role| json!({"name": role.name().as_str()}))
        .collect::<Vec<_>>();
    memberships.sort_by_key(|item| item["name"].as_str().unwrap_or_default().to_string());
    json!({"id": legacy_id(value.id()), "name": value.name().as_str(), "description": value.description(), "roles": memberships, "create_date": value.created_at().date_naive(), "updated_at": value.updated_at()})
}
fn role_json(value: &HostPolicyRole, label_ids: &HashMap<String, u32>) -> Value {
    let labels = value
        .labels()
        .iter()
        .filter_map(|name| label_ids.get(name).copied())
        .collect::<Vec<_>>();
    json!({"id": legacy_id(value.id()), "name": value.name().as_str(), "description": value.description(), "atoms": value.atoms().iter().map(|name| json!({"name": name})).collect::<Vec<_>>(), "hosts": value.hosts().iter().map(|name| json!({"name": name})).collect::<Vec<_>>(), "labels": labels, "create_date": value.created_at().date_naive(), "updated_at": value.updated_at()})
}

async fn policy_roles(state: &AppState) -> Result<Vec<HostPolicyRole>, AppError> {
    Ok(state
        .services
        .host_policy()
        .list_roles(&PageRequest::all())
        .await?
        .items)
}

async fn policy_label_ids(state: &AppState) -> Result<HashMap<String, u32>, AppError> {
    Ok(state
        .services
        .labels()
        .list(&PageRequest::all())
        .await?
        .items
        .into_iter()
        .map(|label| (label.name().as_str().to_string(), legacy_id(label.id())))
        .collect())
}
async fn host_policy_atoms(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::host_policy::atom::LIST,
        actions::resource_kinds::HOST_POLICY_ATOM,
        "*",
    )
    .await?;
    let roles = policy_roles(&state).await?;
    let atoms = state
        .services
        .host_policy()
        .list_atoms(&PageRequest::all())
        .await?
        .items;
    let values = atoms
        .iter()
        .filter(|atom| {
            query.id.is_none_or(|id| legacy_id(atom.id()) == id)
                && query.name_regex.as_ref().is_none_or(|pattern| {
                    let needle = pattern
                        .trim_matches(|character| matches!(character, '.' | '*' | '^' | '$'));
                    atom.name().as_str().contains(needle)
                })
        })
        .map(|atom| atom_json(atom, &roles))
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
async fn host_policy_atom_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::atom::GET,
        actions::resource_kinds::HOST_POLICY_ATOM,
        name.as_str(),
    )
    .await?;
    let atom = state.services.host_policy().get_atom(&name).await?;
    Ok(HttpResponse::Ok().json(atom_json(&atom, &policy_roles(&state).await?)))
}

#[derive(Deserialize)]
struct LegacyCreatePolicyItem {
    name: String,
    description: String,
    #[serde(rename = "create_date")]
    _create_date: Option<String>,
}

#[derive(Deserialize, Default)]
struct LegacyUpdatePolicyAtom {
    name: Option<String>,
    description: Option<String>,
}

async fn create_host_policy_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreatePolicyItem>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = HostPolicyName::new(payload.name)?;
    authorize(
        &req,
        &state,
        actions::host_policy::atom::CREATE,
        actions::resource_kinds::HOST_POLICY_ATOM,
        name.as_str(),
    )
    .await?;
    state
        .services
        .host_policy()
        .create_atom(CreateHostPolicyAtom::new(name.clone(), payload.description))
        .await?;
    Ok(HttpResponse::Created()
        .append_header((
            "Location",
            format!("/api/v1/hostpolicy/atoms/{}", name.as_str()),
        ))
        .finish())
}

async fn update_host_policy_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyUpdatePolicyAtom>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::atom::UPDATE_DESCRIPTION,
        actions::resource_kinds::HOST_POLICY_ATOM,
        name.as_str(),
    )
    .await?;
    let payload = payload.into_inner();
    state
        .services
        .host_policy()
        .update_atom(
            &name,
            UpdateHostPolicyAtom {
                name: payload.name.map(HostPolicyName::new).transpose()?,
                description: payload.description,
            },
        )
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_host_policy_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::atom::DELETE,
        actions::resource_kinds::HOST_POLICY_ATOM,
        name.as_str(),
    )
    .await?;
    state.services.host_policy().delete_atom(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}
async fn host_policy_roles(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::host_policy::role::LIST,
        actions::resource_kinds::HOST_POLICY_ROLE,
        "*",
    )
    .await?;
    let label_ids = policy_label_ids(&state).await?;
    let roles = policy_roles(&state).await?;
    let values = roles
        .iter()
        .filter(|role| {
            query.id.is_none_or(|id| legacy_id(role.id()) == id)
                && query.name_regex.as_ref().is_none_or(|pattern| {
                    let needle = pattern
                        .trim_matches(|character| matches!(character, '.' | '*' | '^' | '$'));
                    role.name().as_str().contains(needle)
                })
                && query
                    .atoms_name_exact
                    .as_ref()
                    .is_none_or(|atom| role.atoms().contains(atom))
        })
        .map(|role| role_json(role, &label_ids))
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
async fn host_policy_role_detail(
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
    Ok(HttpResponse::Ok().json(role_json(&role, &policy_label_ids(&state).await?)))
}

#[derive(Deserialize, Default)]
struct LegacyUpdatePolicyRole {
    name: Option<String>,
    description: Option<String>,
    labels: Option<Vec<u32>>,
}

async fn create_host_policy_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyCreatePolicyItem>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = HostPolicyName::new(payload.name)?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::CREATE,
        actions::resource_kinds::HOST_POLICY_ROLE,
        name.as_str(),
    )
    .await?;
    state
        .services
        .host_policy()
        .create_role(CreateHostPolicyRole::new(name.clone(), payload.description))
        .await?;
    Ok(HttpResponse::Created()
        .append_header((
            "Location",
            format!("/api/v1/hostpolicy/roles/{}", name.as_str()),
        ))
        .finish())
}

async fn update_host_policy_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
    payload: web::Json<LegacyUpdatePolicyRole>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::UPDATE_DESCRIPTION,
        actions::resource_kinds::HOST_POLICY_ROLE,
        name.as_str(),
    )
    .await?;
    let payload = payload.into_inner();
    let role = state
        .services
        .host_policy()
        .update_role(
            &name,
            UpdateHostPolicyRole {
                name: payload.name.map(HostPolicyName::new).transpose()?,
                description: payload.description,
            },
        )
        .await?;
    if let Some(wanted_ids) = payload.labels {
        let labels = state
            .services
            .labels()
            .list(&PageRequest::all())
            .await?
            .items;
        let wanted = labels
            .iter()
            .filter(|label| wanted_ids.contains(&legacy_id(label.id())))
            .map(|label| label.name().as_str().to_string())
            .collect::<Vec<_>>();
        for label in role.labels().iter().filter(|label| !wanted.contains(label)) {
            state
                .services
                .host_policy()
                .remove_label_from_role(role.name(), label)
                .await?;
        }
        for label in wanted.iter().filter(|label| !role.labels().contains(label)) {
            state
                .services
                .host_policy()
                .add_label_to_role(role.name(), label)
                .await?;
        }
    }
    Ok(HttpResponse::NoContent().finish())
}

async fn delete_host_policy_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(name.into_inner())?;
    authorize(
        &req,
        &state,
        actions::host_policy::role::DELETE,
        actions::resource_kinds::HOST_POLICY_ROLE,
        name.as_str(),
    )
    .await?;
    state.services.host_policy().delete_role(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

async fn history(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::audit::HISTORY_LIST,
        actions::resource_kinds::AUDIT_HISTORY,
        "*",
    )
    .await?;
    let events = state
        .services
        .audit()
        .list(&PageRequest::all())
        .await?
        .items;
    let model_ids = query.model_ids.as_ref().map(|values| {
        values
            .split(',')
            .filter_map(|value| value.parse::<u32>().ok())
            .collect::<Vec<_>>()
    });
    let data_ids = query.data_ids.as_ref().map(|values| {
        values
            .split(',')
            .filter_map(|value| value.parse::<u32>().ok())
            .collect::<Vec<_>>()
    });
    let values = events
        .into_iter()
        .filter(|event| {
            query
                .name
                .as_ref()
                .is_none_or(|name| event.resource_name() == name)
                && query.resource.as_ref().is_none_or(|resource| {
                    event.resource_kind() == resource
                        || (resource == "host"
                            && matches!(event.resource_kind(), "host" | "ip_address" | "record"))
                })
                && model_ids.as_ref().is_none_or(|ids| {
                    event
                        .resource_id()
                        .is_some_and(|id| ids.contains(&legacy_id(id)))
                })
                && data_ids.as_ref().is_none_or(|ids| {
                    event
                        .resource_id()
                        .is_some_and(|id| ids.contains(&legacy_id(id)))
                })
        })
        .map(|event| {
            let resource = query
                .resource
                .as_deref()
                .or(query.data_relation.as_deref())
                .unwrap_or_else(|| event.resource_kind());
            let model = event
                .resource_kind()
                .split('_')
                .map(|part| {
                    let mut chars = part.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                        .unwrap_or_default()
                })
                .collect::<String>();
            json!({
                "id": legacy_id(event.id()), "timestamp": event.created_at(),
                "user": event.actor(), "resource": resource,
                "name": event.resource_name(),
                "model_id": event.resource_id().map(legacy_id).unwrap_or(0),
                "model": model, "action": event.action(), "data": event.data(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}

async fn typed_records(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: LegacyPageQuery,
    kind: &'static str,
) -> Result<HttpResponse, AppError> {
    authorize(
        &req,
        &state,
        actions::record::LIST,
        actions::resource_kinds::RECORD,
        kind,
    )
    .await?;
    let host_ids = state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?
        .items
        .into_iter()
        .map(|host| (host.id(), legacy_id(host.id())))
        .collect::<HashMap<_, _>>();
    let matches = |record: &Value, field: &str, expected: Option<&str>| {
        expected.is_none_or(|expected| {
            record.get(field).is_some_and(|actual| match actual {
                Value::String(actual) => actual == expected,
                Value::Number(actual) => actual.to_string() == expected,
                Value::Null => expected.is_empty(),
                _ => false,
            })
        })
    };
    let values = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items
        .iter()
        .filter(|record| record.type_name().as_str() == kind)
        .map(|record| {
            record_json(
                record,
                record.owner_id().and_then(|id| host_ids.get(&id).copied()),
            )
        })
        .filter(|record| {
            matches(
                record,
                "host",
                query.host.map(|value| value.to_string()).as_deref(),
            ) && matches(record, "name", query.name.as_deref())
                && matches(record, "cpu", query.cpu.as_deref())
                && matches(record, "os", query.os.as_deref())
                && matches(record, "loc", query.loc.as_deref())
                && matches(record, "mx", query.mx.as_deref())
                && matches(record, "priority", query.priority.as_deref())
                && matches(record, "order", query.order.as_deref())
                && matches(record, "preference", query.preference.as_deref())
                && matches(record, "flag", query.flag.as_deref())
                && matches(record, "service", query.service.as_deref())
                && matches(record, "regex", query.regex.as_deref())
                && matches(record, "replacement", query.replacement.as_deref())
                && matches(record, "algorithm", query.algorithm.as_deref())
                && matches(record, "hash_type", query.hash_type.as_deref())
                && matches(record, "fingerprint", query.fingerprint.as_deref())
                && matches(record, "port", query.port.as_deref())
                && matches(record, "weight", query.weight.as_deref())
                && matches(record, "ttl", query.ttl.as_deref())
                && matches(record, "txt", query.txt.as_deref())
        })
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query)))
}
macro_rules! record_list {
    ($name:ident, $kind:literal) => {
        async fn $name(
            req: HttpRequest,
            state: web::Data<AppState>,
            query: web::Query<LegacyPageQuery>,
        ) -> Result<HttpResponse, AppError> {
            typed_records(req, state, query.into_inner(), $kind).await
        }
    };
}
record_list!(cnames, "CNAME");
record_list!(hinfos, "HINFO");
record_list!(locs, "LOC");
record_list!(mxs, "MX");
record_list!(naptrs, "NAPTR");
record_list!(sshfps, "SSHFP");
record_list!(srvs, "SRV");
record_list!(txts, "TXT");

#[derive(Deserialize, Default)]
struct LegacyRecordPayload {
    host: Value,
    name: Option<String>,
    cpu: Option<String>,
    os: Option<String>,
    loc: Option<String>,
    mx: Option<String>,
    priority: Option<Value>,
    order: Option<Value>,
    preference: Option<Value>,
    flag: Option<String>,
    service: Option<String>,
    regex: Option<String>,
    replacement: Option<String>,
    algorithm: Option<Value>,
    hash_type: Option<Value>,
    fingerprint: Option<String>,
    weight: Option<Value>,
    port: Option<Value>,
    txt: Option<String>,
}

fn legacy_u32(value: &Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn required_u32(value: Option<&Value>, field: &str) -> Result<u32, AppError> {
    value
        .and_then(legacy_u32)
        .ok_or_else(|| AppError::validation(format!("{field} must be an integer")))
}

async fn legacy_record_host(
    state: &AppState,
    value: &Value,
) -> Result<crate::domain::host::Host, AppError> {
    let id = legacy_u32(value).ok_or_else(|| AppError::validation("host must be an integer"))?;
    state
        .services
        .hosts()
        .list(&PageRequest::all(), &HostFilter::default())
        .await?
        .items
        .into_iter()
        .find(|host| legacy_id(host.id()) == id)
        .ok_or_else(|| AppError::not_found("host was not found"))
}

fn parse_legacy_loc(value: &str) -> Result<Value, AppError> {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 9 {
        return Err(AppError::validation("LOC value is malformed"));
    }
    let coordinate = |degree: &str, minute: &str, second: &str, direction: &str| {
        let degree = degree
            .parse::<f64>()
            .map_err(|_| AppError::validation("LOC degree is invalid"))?;
        let minute = minute
            .parse::<f64>()
            .map_err(|_| AppError::validation("LOC minute is invalid"))?;
        let second = second
            .parse::<f64>()
            .map_err(|_| AppError::validation("LOC second is invalid"))?;
        let sign = if matches!(direction, "S" | "W") {
            -1.0
        } else {
            1.0
        };
        Ok::<f64, AppError>(sign * (degree + minute / 60.0 + second / 3600.0))
    };
    let metres = |part: &str| {
        part.trim_end_matches('m')
            .parse::<f64>()
            .map_err(|_| AppError::validation("LOC distance is invalid"))
    };
    let latitude = coordinate(parts[0], parts[1], parts[2], parts[3])?;
    let longitude = coordinate(parts[4], parts[5], parts[6], parts[7])?;
    let mut data =
        json!({"latitude": latitude, "longitude": longitude, "altitude_m": metres(parts[8])?});
    for (index, key) in [
        (9, "size_m"),
        (10, "horizontal_precision_m"),
        (11, "vertical_precision_m"),
    ] {
        if let Some(part) = parts.get(index) {
            let value = metres(part)?;
            data[key] = json!(if key == "size_m" && value == 0.0 {
                0.001
            } else {
                value
            });
        }
    }
    Ok(data)
}

fn legacy_loc_string(data: &Value) -> String {
    let coordinate = |value: f64, positive: char, negative: char| {
        let direction = if value < 0.0 { negative } else { positive };
        let absolute = value.abs();
        let degrees = absolute.floor();
        let minutes_value = (absolute - degrees) * 60.0;
        let minutes = minutes_value.floor();
        let seconds = (minutes_value - minutes) * 60.0;
        format!("{degrees:.0} {minutes:.0} {seconds:.3} {direction}")
    };
    format!(
        "{} {} {:.2}m {:.2}m {:.0}m {:.0}m",
        coordinate(data["latitude"].as_f64().unwrap_or_default(), 'N', 'S'),
        coordinate(data["longitude"].as_f64().unwrap_or_default(), 'E', 'W'),
        data["altitude_m"].as_f64().unwrap_or_default(),
        data["size_m"].as_f64().unwrap_or(1.0),
        data["horizontal_precision_m"].as_f64().unwrap_or(10_000.0),
        data["vertical_precision_m"].as_f64().unwrap_or(10.0),
    )
}

async fn create_legacy_record(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<LegacyRecordPayload>,
    kind: &'static str,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let host = legacy_record_host(&state, &payload.host).await?;
    authorize(
        &req,
        &state,
        actions::record::CREATE_ANCHORED,
        actions::resource_kinds::RECORD,
        host.name().as_str(),
    )
    .await?;
    if kind == "SRV" {
        let name = payload.name.as_deref().unwrap_or_default();
        if !(name.starts_with('_')
            && (name.contains("._tcp.") || name.contains("._udp.") || name.contains("._tls.")))
        {
            return Ok(HttpResponse::BadRequest().json(json!({"type":"validation_error","errors":[{"code":"invalid","detail":"Must match: ^_[a-z0-9]+(([a-z0-9][_-]?)+[a-z0-9]+)?._(tcp|tls|udp)\\.","attr":"name"}]})));
        }
    }
    let data = match kind {
        "CNAME" => json!({"target": host.name().as_str()}),
        "HINFO" => {
            json!({"cpu": payload.cpu.as_deref().unwrap_or_default(), "os": payload.os.as_deref().unwrap_or_default()})
        }
        "LOC" => parse_legacy_loc(payload.loc.as_deref().unwrap_or_default())?,
        "MX" => {
            json!({"preference": required_u32(payload.priority.as_ref(), "priority")?, "exchange": payload.mx.as_deref().unwrap_or_default()})
        }
        "NAPTR" => {
            json!({"order": required_u32(payload.order.as_ref(), "order")?, "preference": required_u32(payload.preference.as_ref(), "preference")?, "flags": payload.flag.as_deref().unwrap_or_default(), "services": format!("\u{1f}mreg-v1:{}", payload.service.as_deref().unwrap_or_default()), "regexp": payload.regex.as_deref().unwrap_or_default(), "replacement": payload.replacement.as_deref().unwrap_or_default()})
        }
        "SSHFP" => {
            json!({"algorithm": required_u32(payload.algorithm.as_ref(), "algorithm")?, "fp_type": required_u32(payload.hash_type.as_ref(), "hash_type")?, "fingerprint": format!("\u{1f}mreg-v1:{}", payload.fingerprint.as_deref().unwrap_or_default())})
        }
        "SRV" => {
            json!({"priority": required_u32(payload.priority.as_ref(), "priority")?, "weight": required_u32(payload.weight.as_ref(), "weight")?, "port": required_u32(payload.port.as_ref(), "port")?, "target": host.name().as_str()})
        }
        "TXT" => json!({"value": payload.txt.as_deref().unwrap_or_default()}),
        _ => return Err(AppError::validation("unsupported legacy record type")),
    };
    let type_name = RecordTypeName::new(kind)?;
    let command = if matches!(kind, "CNAME" | "SRV") {
        CreateRecordInstance::new_anchored(
            type_name,
            RecordOwnerKind::Host,
            payload
                .name
                .unwrap_or_else(|| host.name().as_str().to_string()),
            host.name().as_str(),
            None,
            data,
        )?
    } else {
        CreateRecordInstance::new(
            type_name,
            RecordOwnerKind::Host,
            host.name().as_str(),
            None,
            data,
        )?
    };
    let record = state.services.records().create_record(command).await?;
    Ok(HttpResponse::Created().json(record_json(&record, Some(legacy_id(host.id())))))
}

macro_rules! record_create {
    ($name:ident, $kind:literal) => {
        async fn $name(
            req: HttpRequest,
            state: web::Data<AppState>,
            payload: web::Json<LegacyRecordPayload>,
        ) -> Result<HttpResponse, AppError> {
            create_legacy_record(req, state, payload, $kind).await
        }
    };
}
record_create!(create_cname, "CNAME");
record_create!(create_hinfo, "HINFO");
record_create!(create_loc, "LOC");
record_create!(create_mx, "MX");
record_create!(create_naptr, "NAPTR");
record_create!(create_sshfp, "SSHFP");
record_create!(create_srv, "SRV");
record_create!(create_txt, "TXT");

async fn delete_legacy_record_by_id(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: u32,
    kind: &'static str,
) -> Result<HttpResponse, AppError> {
    let record = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items
        .into_iter()
        .find(|record| {
            record.type_name().as_str() == kind
                && (legacy_id(record.id()) == id
                    || (matches!(kind, "HINFO" | "LOC")
                        && record
                            .owner_id()
                            .is_some_and(|owner| legacy_id(owner) == id)))
        })
        .ok_or_else(|| AppError::not_found("record was not found"))?;
    authorize(
        &req,
        &state,
        actions::record::DELETE,
        actions::resource_kinds::RECORD,
        &record.id().to_string(),
    )
    .await?;
    state.services.records().delete_record(record.id()).await?;
    Ok(HttpResponse::NoContent().finish())
}

macro_rules! record_delete {
    ($name:ident, $kind:literal) => {
        async fn $name(
            req: HttpRequest,
            state: web::Data<AppState>,
            id: web::Path<u32>,
        ) -> Result<HttpResponse, AppError> {
            delete_legacy_record_by_id(req, state, id.into_inner(), $kind).await
        }
    };
}
record_delete!(delete_hinfo, "HINFO");
record_delete!(delete_loc, "LOC");
record_delete!(delete_mx, "MX");
record_delete!(delete_naptr, "NAPTR");
record_delete!(delete_sshfp, "SSHFP");
record_delete!(delete_srv, "SRV");
record_delete!(delete_txt, "TXT");

async fn hinfo_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    id: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let id = id.into_inner();
    let record = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items
        .into_iter()
        .find(|record| {
            record.type_name().as_str() == "HINFO"
                && record
                    .owner_id()
                    .is_some_and(|owner| legacy_id(owner) == id)
        })
        .ok_or_else(|| AppError::not_found("HINFO was not found"))?;
    authorize(
        &req,
        &state,
        actions::record::GET,
        actions::resource_kinds::RECORD,
        record.owner_name(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(record_json(&record, Some(id))))
}

async fn cname_detail(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = name.into_inner();
    authorize(
        &req,
        &state,
        actions::record::GET,
        actions::resource_kinds::RECORD,
        &name,
    )
    .await?;
    let item = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items
        .into_iter()
        .find(|record| record.type_name().as_str() == "CNAME" && record.owner_name() == name)
        .ok_or_else(|| AppError::not_found("CNAME was not found"))?;
    Ok(HttpResponse::Ok().json(record_json(&item, item.owner_id().map(legacy_id))))
}

async fn delete_cname(
    req: HttpRequest,
    state: web::Data<AppState>,
    name: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = name.into_inner();
    let record = state
        .services
        .records()
        .list_records(&PageRequest::all(), &RecordFilter::default())
        .await?
        .items
        .into_iter()
        .find(|record| record.type_name().as_str() == "CNAME" && record.owner_name() == name)
        .ok_or_else(|| AppError::not_found("CNAME was not found"))?;
    authorize(
        &req,
        &state,
        actions::record::DELETE,
        actions::resource_kinds::RECORD,
        &record.id().to_string(),
    )
    .await?;
    state.services.records().delete_record(record.id()).await?;
    Ok(HttpResponse::NoContent().finish())
}

fn community_json(value: &crate::domain::community::Community, hosts: Vec<String>) -> Value {
    json!({"id": value.id(), "name": value.name().as_str(), "description": value.description(), "network": value.network_cidr().as_str(), "hosts": hosts, "global_name": null, "created_at": value.created_at(), "updated_at": value.updated_at()})
}
async fn network_communities(
    req: HttpRequest,
    state: web::Data<AppState>,
    network: web::Path<String>,
    query: web::Query<LegacyPageQuery>,
) -> Result<HttpResponse, AppError> {
    let network = crate::domain::types::CidrValue::new(network.into_inner())?;
    authorize(
        &req,
        &state,
        actions::community::LIST,
        actions::resource_kinds::COMMUNITY,
        &network.as_str(),
    )
    .await?;
    let communities = state
        .services
        .communities()
        .list(&PageRequest::all(), &CommunityFilter::default())
        .await?
        .items;
    let assignments = state
        .services
        .host_community_assignments()
        .list(&PageRequest::all(), &Default::default())
        .await?
        .items;
    let values = communities
        .iter()
        .filter(|community| community.network_cidr() == &network)
        .map(|community| {
            community_json(
                community,
                assignments
                    .iter()
                    .filter(|mapping| mapping.community_id() == community.id())
                    .map(|mapping| mapping.host_name().as_str().to_string())
                    .collect(),
            )
        })
        .collect();
    Ok(HttpResponse::Ok().json(paginate(values, query.into_inner())))
}
