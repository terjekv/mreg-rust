use std::collections::{BTreeMap, HashMap};

use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, AuthorizationRequest},
    domain::{
        filters::HostFilter,
        filters::{
            BacnetIdFilter, FilterCondition, FilterOp, HostContactFilter, HostGroupFilter,
            RecordFilter,
        },
        host::{
            AllocationPolicy, AssignIpAddress, CreateHost, Host, IpAddressAssignment,
            IpAssignmentSpec, UpdateHost, UpdateIpAddress,
        },
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{CidrValue, Hostname, IpAddressValue, MacAddressValue, Ttl, UpdateField, ZoneName},
    },
    errors::AppError,
};

use super::authz::{
    UpdateAuthzBuilder, host_attrs_for_host, host_request as host_authz_request,
    request as authz_request, require, require_all,
};
use super::{
    attachment_community_assignments::AttachmentCommunityAssignmentResponse,
    attachments::{AttachmentDhcpIdentifierResponse, AttachmentPrefixReservationResponse},
};

crate::page_response!(HostPageResponse, HostResponse, "Paginated list of hosts.");
crate::page_response!(
    IpAddressPageResponse,
    IpAddressResponse,
    "Paginated list of IP address assignments."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_hosts)
        .service(create_host)
        .service(get_host)
        .service(update_host)
        .service(delete_host)
        .service(list_ip_addresses)
        .service(list_host_ip_addresses)
        .service(assign_ip_address)
        .service(update_ip_address)
        .service(unassign_ip_address);
}

#[derive(Deserialize)]
pub struct ListHostsQuery {
    // Pagination + sort
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    // Special filter fields
    search: Option<String>,
    // Operator-based filter params
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl ListHostsQuery {
    fn into_parts(self) -> Result<(PageRequest, HostFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let mut filter = HostFilter::from_query_params(self.filters)?;
        filter.search = self.search;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct IpAssignmentRequest {
    address: Option<String>,
    network: Option<String>,
    #[serde(default)]
    allocation: Option<String>,
    mac_address: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateHostRequest {
    name: String,
    zone: Option<String>,
    ttl: Option<u32>,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    ip_addresses: Vec<IpAssignmentRequest>,
}

impl CreateHostRequest {
    fn into_command(
        self,
        auto_v4_client_id: bool,
        auto_v6_duid_ll: bool,
    ) -> Result<CreateHost, AppError> {
        let mut specs = Vec::with_capacity(self.ip_addresses.len());
        for ip_req in self.ip_addresses {
            let allocation = match ip_req.allocation.as_deref() {
                Some("random") => AllocationPolicy::Random,
                Some("first_free") | None => AllocationPolicy::FirstFree,
                Some(other) => {
                    return Err(AppError::validation(format!(
                        "unknown allocation policy: {other}"
                    )));
                }
            };
            specs.push(
                IpAssignmentSpec::new(
                    ip_req.address.map(IpAddressValue::new).transpose()?,
                    ip_req.network.map(CidrValue::new).transpose()?,
                    allocation,
                    ip_req.mac_address.map(MacAddressValue::new).transpose()?,
                )?
                .with_auto_dhcp(auto_v4_client_id, auto_v6_duid_ll),
            );
        }
        let cmd = CreateHost::new(
            Hostname::new(self.name)?,
            self.zone.map(ZoneName::new).transpose()?,
            self.ttl.map(Ttl::new).transpose()?,
            self.comment,
        )?;
        Ok(cmd.with_ip_assignments(specs))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct AssignIpAddressRequest {
    host_name: String,
    address: Option<String>,
    network: Option<String>,
    mac_address: Option<String>,
}

impl AssignIpAddressRequest {
    fn into_command(self) -> Result<AssignIpAddress, AppError> {
        AssignIpAddress::new(
            Hostname::new(self.host_name)?,
            self.address.map(IpAddressValue::new).transpose()?,
            self.network.map(CidrValue::new).transpose()?,
            self.mac_address.map(MacAddressValue::new).transpose()?,
        )
    }
}

#[derive(Serialize, ToSchema)]
pub struct HostResponse {
    id: Uuid,
    name: String,
    zone: Option<String>,
    ttl: Option<u32>,
    comment: String,
    attachments: Vec<HostAttachmentInventoryResponse>,
    inventory: HostInventorySummary,
    dns_records: Vec<HostDnsRecordSummary>,
    host_policy: HostPolicySummary,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostResponse {
    fn from_domain(host: &Host) -> Self {
        Self {
            id: host.id(),
            name: host.name().as_str().to_string(),
            zone: host.zone().map(|zone| zone.as_str().to_string()),
            ttl: host.ttl().map(|ttl| ttl.as_u32()),
            comment: host.comment().to_string(),
            attachments: Vec::new(),
            inventory: HostInventorySummary::default(),
            dns_records: Vec::new(),
            host_policy: HostPolicySummary::default(),
            created_at: host.created_at(),
            updated_at: host.updated_at(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct HostAttachmentInventoryResponse {
    id: Uuid,
    network_id: Uuid,
    network: String,
    mac_address: Option<String>,
    comment: Option<String>,
    ip_addresses: Vec<IpAddressResponse>,
    dhcp_identifiers: Vec<AttachmentDhcpIdentifierResponse>,
    prefix_reservations: Vec<AttachmentPrefixReservationResponse>,
    community_assignments: Vec<AttachmentCommunityAssignmentResponse>,
}

#[derive(Serialize, ToSchema, Default)]
pub struct HostInventorySummary {
    contacts: Vec<String>,
    groups: Vec<String>,
    bacnet_id: Option<u32>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct HostDnsRecordSummary {
    id: Uuid,
    type_name: String,
    ttl: Option<u32>,
    rendered: Option<String>,
}

#[derive(Serialize, ToSchema, Default)]
pub struct HostPolicySummary {
    roles: Vec<String>,
    atoms: Vec<String>,
}

// `pub` only with `bench-helpers` so benches can call this read-model builder.
// Production library builds keep it crate-internal.
#[cfg(feature = "bench-helpers")]
pub async fn build_host_response(
    state: &AppState,
    host: &Host,
    include_details: bool,
) -> Result<HostResponse, AppError> {
    build_host_response_impl(state, host, include_details).await
}

#[cfg(not(feature = "bench-helpers"))]
pub(crate) async fn build_host_response(
    state: &AppState,
    host: &Host,
    include_details: bool,
) -> Result<HostResponse, AppError> {
    build_host_response_impl(state, host, include_details).await
}

async fn build_host_response_impl(
    state: &AppState,
    host: &Host,
    include_details: bool,
) -> Result<HostResponse, AppError> {
    let mut response = HostResponse::from_domain(host);
    if !include_details {
        return Ok(response);
    }

    let attachments = state
        .services
        .attachments()
        .list_attachments_for_host(host.name())
        .await?;
    let all_ip_addresses = state
        .services
        .hosts()
        .list_host_ip_addresses(host.name(), &PageRequest::all())
        .await?;
    let attachment_ids = attachments
        .iter()
        .map(|attachment| attachment.id())
        .collect::<Vec<_>>();
    let all_attachment_assignments = state
        .services
        .attachments()
        .list_attachment_community_assignments_for_attachments(&attachment_ids)
        .await?;
    let all_dhcp_identifiers = state
        .services
        .attachments()
        .list_attachment_dhcp_identifiers_for_attachments(&attachment_ids)
        .await?;
    let all_prefix_reservations = state
        .services
        .attachments()
        .list_attachment_prefix_reservations_for_attachments(&attachment_ids)
        .await?;
    let ip_addresses_by_attachment = all_ip_addresses.items.iter().fold(
        HashMap::<Uuid, Vec<IpAddressResponse>>::new(),
        |mut acc, assignment| {
            acc.entry(assignment.attachment_id())
                .or_default()
                .push(IpAddressResponse::from_domain(assignment));
            acc
        },
    );
    let dhcp_by_attachment = all_dhcp_identifiers.iter().fold(
        HashMap::<Uuid, Vec<AttachmentDhcpIdentifierResponse>>::new(),
        |mut acc, identifier| {
            acc.entry(identifier.attachment_id())
                .or_default()
                .push(AttachmentDhcpIdentifierResponse::from_domain(identifier));
            acc
        },
    );
    let prefixes_by_attachment = all_prefix_reservations.iter().fold(
        HashMap::<Uuid, Vec<AttachmentPrefixReservationResponse>>::new(),
        |mut acc, reservation| {
            acc.entry(reservation.attachment_id()).or_default().push(
                AttachmentPrefixReservationResponse::from_domain(reservation),
            );
            acc
        },
    );
    let assignments_by_attachment = all_attachment_assignments.iter().fold(
        HashMap::<Uuid, Vec<AttachmentCommunityAssignmentResponse>>::new(),
        |mut acc, assignment| {
            acc.entry(assignment.attachment_id()).or_default().push(
                AttachmentCommunityAssignmentResponse::from_domain(assignment),
            );
            acc
        },
    );
    let mut attachment_responses = Vec::with_capacity(attachments.len());
    for attachment in &attachments {
        attachment_responses.push(HostAttachmentInventoryResponse {
            id: attachment.id(),
            network_id: attachment.network_id(),
            network: attachment.network_cidr().as_str(),
            mac_address: attachment.mac_address().map(|value| value.as_str()),
            comment: attachment.comment().map(str::to_string),
            ip_addresses: ip_addresses_by_attachment
                .get(&attachment.id())
                .cloned()
                .unwrap_or_default(),
            dhcp_identifiers: dhcp_by_attachment
                .get(&attachment.id())
                .cloned()
                .unwrap_or_default(),
            prefix_reservations: prefixes_by_attachment
                .get(&attachment.id())
                .cloned()
                .unwrap_or_default(),
            community_assignments: assignments_by_attachment
                .get(&attachment.id())
                .cloned()
                .unwrap_or_default(),
        });
    }
    response.attachments = attachment_responses;

    let exact_host = FilterCondition {
        op: FilterOp::Equals,
        value: host.name().as_str().to_string(),
    };
    response.inventory.contacts = state
        .services
        .host_contacts()
        .list(
            &PageRequest::all(),
            &HostContactFilter {
                host: vec![exact_host.clone()],
                ..HostContactFilter::default()
            },
        )
        .await?
        .items
        .into_iter()
        .map(|contact| contact.email().as_str().to_string())
        .collect();
    response.inventory.groups = state
        .services
        .host_groups()
        .list(
            &PageRequest::all(),
            &HostGroupFilter {
                host: vec![exact_host.clone()],
                ..HostGroupFilter::default()
            },
        )
        .await?
        .items
        .into_iter()
        .map(|group| group.name().as_str().to_string())
        .collect();
    response.inventory.bacnet_id = state
        .services
        .bacnet()
        .list(
            &PageRequest::all(),
            &BacnetIdFilter {
                host: vec![exact_host.clone()],
                ..BacnetIdFilter::default()
            },
        )
        .await?
        .items
        .into_iter()
        .next()
        .map(|assignment| assignment.bacnet_id().as_u32());

    response.dns_records = state
        .services
        .records()
        .list_records(
            &PageRequest::all(),
            &RecordFilter {
                owner_kind: vec![FilterCondition {
                    op: FilterOp::Equals,
                    value: "host".to_string(),
                }],
                owner_name: vec![exact_host.clone()],
                ..RecordFilter::default()
            },
        )
        .await?
        .items
        .into_iter()
        .map(|record| HostDnsRecordSummary {
            id: record.id(),
            type_name: record.type_name().as_str().to_string(),
            ttl: record.ttl().map(|ttl| ttl.as_u32()),
            rendered: record.rendered().map(str::to_string),
        })
        .collect();

    let roles = state
        .services
        .host_policy()
        .list_roles_for_host(host.name())
        .await?;
    let mut atoms = std::collections::BTreeSet::new();
    response.host_policy.roles = roles
        .into_iter()
        .map(|role| {
            for atom in role.atoms() {
                atoms.insert(atom.to_string());
            }
            role.name().as_str().to_string()
        })
        .collect();
    response.host_policy.atoms = atoms.into_iter().collect();

    Ok(response)
}

#[derive(Clone, Serialize, ToSchema)]
pub struct IpAddressResponse {
    id: Uuid,
    host_id: Uuid,
    address: String,
    family: u8,
    network_id: Uuid,
    mac_address: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl IpAddressResponse {
    pub fn from_domain(assignment: &IpAddressAssignment) -> Self {
        Self {
            id: assignment.id(),
            host_id: assignment.host_id(),
            address: assignment.address().as_str(),
            family: assignment.family(),
            network_id: assignment.network_id(),
            mac_address: assignment.mac_address().map(|value| value.as_str()),
            created_at: assignment.created_at(),
            updated_at: assignment.updated_at(),
        }
    }
}

/// List hosts with optional filters
#[utoipa::path(
    get,
    path = "/api/v1/inventory/hosts",
    responses(
        (status = 200, description = "Paginated list of hosts", body = HostPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/hosts")]
pub(crate) async fn list_hosts(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ListHostsQuery>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            authz::actions::host::LIST,
            authz::actions::resource_kinds::HOST,
            "*",
        ),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state.services.hosts().list(&page, &filter).await?;
    let mut items = Vec::with_capacity(result.items.len());
    for host in &result.items {
        items.push(build_host_response(state.get_ref(), host, false).await?);
    }
    Ok(HttpResponse::Ok().json(PageResponse {
        items,
        total: result.total,
        next_cursor: result.next_cursor,
    }))
}

/// Create a new host
#[utoipa::path(
    post,
    path = "/api/v1/inventory/hosts",
    request_body = CreateHostRequest,
    responses(
        (status = 201, description = "Host created", body = HostResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Host already exists")
    ),
    tag = "Inventory"
)]
#[post("/inventory/hosts")]
pub(crate) async fn create_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateHostRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::host::CREATE,
        authz::actions::resource_kinds::HOST,
        request.name.clone(),
    )
    .attr("name", AttrValue::String(request.name.clone()))
    .attr("labels", AttrValue::Set(Vec::new()))
    .attr("host_groups", AttrValue::Set(Vec::new()))
    .attr("addresses", AttrValue::Set(Vec::new()))
    .attr("networks", AttrValue::Set(Vec::new()));
    if let Some(zone) = &request.zone {
        authz = authz.attr("zone", AttrValue::String(zone.clone()));
    }
    if let Some(ttl) = request.ttl {
        authz = authz.attr("ttl", AttrValue::Long(i64::from(ttl)));
    }
    require(&state, authz).await?;

    let auto_v4 = state.config.dhcp_auto_v4_client_id;
    let auto_v6 = state.config.dhcp_auto_v6_duid_ll;
    let host = state
        .services
        .hosts()
        .create(request.into_command(auto_v4, auto_v6)?)
        .await?;
    Ok(HttpResponse::Created().json(build_host_response(state.get_ref(), &host, false).await?))
}

/// Get a host by name
#[utoipa::path(
    get,
    path = "/api/v1/inventory/hosts/{name}",
    params(("name" = String, Path, description = "Hostname (FQDN)")),
    responses(
        (status = 200, description = "Host found", body = HostResponse),
        (status = 404, description = "Host not found")
    ),
    tag = "Inventory"
)]
#[get("/inventory/hosts/{name}")]
pub(crate) async fn get_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = Hostname::new(path.into_inner())?;
    require(
        &state,
        host_authz_request(state.get_ref(), &req, authz::actions::host::GET, &name).await?,
    )
    .await?;
    let host = state.services.hosts().get(&name).await?;
    Ok(HttpResponse::Ok().json(build_host_response(state.get_ref(), &host, true).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateHostRequest {
    name: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<u32>)]
    ttl: UpdateField<u32>,
    comment: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    zone: UpdateField<String>,
}

fn build_host_update_authz(
    req: &HttpRequest,
    name: &str,
    request: &UpdateHostRequest,
    base_attrs: BTreeMap<String, AttrValue>,
) -> Vec<AuthorizationRequest> {
    let mut b = UpdateAuthzBuilder::new(req, authz::actions::resource_kinds::HOST, name)
        .with_base_attrs(base_attrs);
    b.field_string(&request.name, authz::actions::host::UPDATE_NAME, "new_name")
        .field_clearable(
            &request.ttl,
            authz::actions::host::UPDATE_TTL,
            "new_ttl",
            "clear_ttl",
            |v| AttrValue::Long(i64::from(*v)),
        )
        .field_present(&request.comment, authz::actions::host::UPDATE_COMMENT)
        .field_clearable(
            &request.zone,
            authz::actions::host::UPDATE_ZONE,
            "new_zone",
            "clear_zone",
            |v| AttrValue::String(v.clone()),
        );
    b.build()
}

/// Update a host
#[utoipa::path(
    patch,
    path = "/api/v1/inventory/hosts/{name}",
    params(("name" = String, Path, description = "Hostname (FQDN)")),
    request_body = UpdateHostRequest,
    responses(
        (status = 200, description = "Host updated", body = HostResponse),
        (status = 404, description = "Host not found")
    ),
    tag = "Inventory"
)]
#[patch("/inventory/hosts/{name}")]
pub(crate) async fn update_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateHostRequest>,
) -> Result<HttpResponse, AppError> {
    let current_name = Hostname::new(path.into_inner())?;
    let request = payload.into_inner();
    let base_attrs = host_attrs_for_host(state.get_ref(), &current_name).await?;
    let authz_requests = build_host_update_authz(&req, current_name.as_str(), &request, base_attrs);
    require_all(&state, authz_requests).await?;

    let name = request.name.map(Hostname::new).transpose()?;
    let ttl = request.ttl.try_map(Ttl::new)?;
    let zone = request.zone.try_map(ZoneName::new)?;
    let command = UpdateHost {
        name,
        ttl,
        comment: request.comment,
        zone,
    };
    let host = state
        .services
        .hosts()
        .update(&current_name, command)
        .await?;
    Ok(HttpResponse::Ok().json(build_host_response(state.get_ref(), &host, false).await?))
}

/// Delete a host
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/hosts/{name}",
    params(("name" = String, Path, description = "Hostname (FQDN)")),
    responses(
        (status = 204, description = "Host deleted"),
        (status = 404, description = "Host not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/hosts/{name}")]
pub(crate) async fn delete_host(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = Hostname::new(path.into_inner())?;
    require(
        &state,
        host_authz_request(state.get_ref(), &req, authz::actions::host::DELETE, &name).await?,
    )
    .await?;
    state.services.hosts().delete(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

/// List all IP address assignments
#[utoipa::path(
    get,
    path = "/api/v1/inventory/ip-addresses",
    responses(
        (status = 200, description = "List of IP address assignments", body = IpAddressPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/ip-addresses")]
pub(crate) async fn list_ip_addresses(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            authz::actions::host::ip::LIST,
            authz::actions::resource_kinds::IP_ADDRESS,
            "*",
        ),
    )
    .await?;
    let page = state
        .services
        .hosts()
        .list_ip_addresses(&PageRequest::all())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        IpAddressResponse::from_domain,
    )))
}

/// List IP addresses for a host
#[utoipa::path(
    get,
    path = "/api/v1/inventory/hosts/{name}/ip-addresses",
    params(("name" = String, Path, description = "Hostname (FQDN)")),
    responses(
        (status = 200, description = "List of IP address assignments for host", body = IpAddressPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/hosts/{name}/ip-addresses")]
pub(crate) async fn list_host_ip_addresses(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = Hostname::new(path.into_inner())?;
    require(
        &state,
        host_authz_request(
            state.get_ref(),
            &req,
            authz::actions::host::ip::LIST_FOR_HOST,
            &name,
        )
        .await?,
    )
    .await?;
    let page = state
        .services
        .hosts()
        .list_host_ip_addresses(&name, &PageRequest::all())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        IpAddressResponse::from_domain,
    )))
}

/// Assign an IP address to a host
#[utoipa::path(
    post,
    path = "/api/v1/inventory/ip-addresses",
    request_body = AssignIpAddressRequest,
    responses(
        (status = 201, description = "IP address assigned", body = IpAddressResponse),
        (status = 400, description = "Validation error")
    ),
    tag = "Inventory"
)]
#[post("/inventory/ip-addresses")]
pub(crate) async fn assign_ip_address(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<AssignIpAddressRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let (action, resource_kind, resource_id) = match (&request.address, &request.network) {
        (Some(address), _) => (
            authz::actions::host::ip::ASSIGN_MANUAL,
            authz::actions::resource_kinds::IP_ADDRESS,
            address.clone(),
        ),
        (None, Some(network)) => (
            authz::actions::host::ip::ASSIGN_AUTO,
            authz::actions::resource_kinds::NETWORK,
            network.clone(),
        ),
        (None, None) => (
            authz::actions::host::ip::ASSIGN_AUTO,
            authz::actions::resource_kinds::HOST,
            request.host_name.clone(),
        ),
    };
    let mut authz = authz_request(&req, action, resource_kind, resource_id)
        .attr("host_name", AttrValue::String(request.host_name.clone()));
    if let Some(address) = &request.address {
        authz = authz.attr("address", AttrValue::Ip(address.clone()));
    }
    if let Some(network) = &request.network {
        authz = authz.attr("network", AttrValue::Ip(network.clone()));
    }
    if let Some(mac_address) = &request.mac_address {
        authz = authz.attr("mac_address", AttrValue::String(mac_address.clone()));
    }
    require(&state, authz).await?;

    let auto_v4 = state.config.dhcp_auto_v4_client_id;
    let auto_v6 = state.config.dhcp_auto_v6_duid_ll;
    let assignment = state
        .services
        .hosts()
        .assign_ip_address(request.into_command()?.with_auto_dhcp(auto_v4, auto_v6))
        .await?;
    Ok(HttpResponse::Created().json(IpAddressResponse::from_domain(&assignment)))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateIpAddressRequest {
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    mac_address: UpdateField<String>,
}

/// Update an IP address assignment
#[utoipa::path(
    patch,
    path = "/api/v1/inventory/ip-addresses/{address}",
    params(("address" = String, Path, description = "IP address")),
    request_body = UpdateIpAddressRequest,
    responses(
        (status = 200, description = "IP address updated", body = IpAddressResponse),
        (status = 404, description = "IP address not found")
    ),
    tag = "Inventory"
)]
#[patch("/inventory/ip-addresses/{address}")]
pub(crate) async fn update_ip_address(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateIpAddressRequest>,
) -> Result<HttpResponse, AppError> {
    let address = IpAddressValue::new(path.into_inner())?;
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::host::ip::UPDATE_MAC,
        authz::actions::resource_kinds::IP_ADDRESS,
        address.as_str(),
    );
    if let UpdateField::Set(ref mac_address) = request.mac_address {
        authz = authz.attr("new_mac_address", AttrValue::String(mac_address.clone()));
    }
    require(&state, authz).await?;
    let mac = request.mac_address.try_map(MacAddressValue::new)?;
    let command = UpdateIpAddress { mac_address: mac };
    let assignment = state
        .services
        .hosts()
        .update_ip_address(&address, command)
        .await?;
    Ok(HttpResponse::Ok().json(IpAddressResponse::from_domain(&assignment)))
}

/// Unassign an IP address
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/ip-addresses/{address}",
    params(("address" = String, Path, description = "IP address")),
    responses(
        (status = 204, description = "IP address unassigned"),
        (status = 404, description = "IP address not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/ip-addresses/{address}")]
pub(crate) async fn unassign_ip_address(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let address = IpAddressValue::new(path.into_inner())?;
    require(
        &state,
        authz_request(
            &req,
            authz::actions::host::ip::UNASSIGN,
            authz::actions::resource_kinds::IP_ADDRESS,
            address.as_str(),
        ),
    )
    .await?;
    state.services.hosts().unassign_ip_address(&address).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn auto_allocate_ip_skips_reserved_and_excluded_ranges() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        for request in [
            serde_json::json!({"cidr":"10.0.0.0/24","description":"LAN","reserved":5}),
            serde_json::json!({"cidr":"10.0.1.0/24","description":"LAN 2","reserved":3}),
        ] {
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/inventory/networks")
                    .set_json(request)
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        let excluded = test::TestRequest::post()
            .uri("/inventory/networks/excluded-ranges")
            .set_json(serde_json::json!({
                "network": "10.0.0.0/24",
                "start_ip": "10.0.0.5",
                "end_ip": "10.0.0.10",
                "description": "Reserved"
            }))
            .to_request();
        let response = test::call_service(&app, excluded).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({
                    "name": "app.example.org",
                    "comment": "App host"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/ip-addresses")
                .set_json(serde_json::json!({
                    "host_name": "app.example.org",
                    "network": "10.0.0.0/24"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["address"], "10.0.0.11");
    }

    #[actix_web::test]
    async fn list_hosts_supports_zone_and_address_filters() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/nameservers")
                .set_json(serde_json::json!({
                    "name": "ns1.example.org"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/forward-zones")
                .set_json(serde_json::json!({
                    "name": "example.org",
                    "primary_ns": "ns1.example.org",
                    "nameservers": ["ns1.example.org"],
                    "email": "hostmaster@example.org",
                    "serial_no": 1,
                    "refresh": 10800,
                    "retry": 3600,
                    "expire": 1814400,
                    "soa_ttl": 43200,
                    "default_ttl": 43200
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/networks")
                .set_json(serde_json::json!({
                    "cidr": "10.0.0.0/24",
                    "description": "LAN",
                    "reserved": 3
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        for request in [
            serde_json::json!({"name":"api.example.org","zone":"example.org","comment":"api"}),
            serde_json::json!({"name":"db.example.org","zone":"example.org","comment":"db"}),
        ] {
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/inventory/hosts")
                    .set_json(request)
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/ip-addresses")
                .set_json(serde_json::json!({
                    "host_name": "api.example.org",
                    "address": "10.0.0.50"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/inventory/hosts?zone=example.org&address=10.0.0.50")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
        assert_eq!(body["items"][0]["name"], "api.example.org");
    }

    #[actix_web::test]
    async fn ip_assignment_auto_creates_a_record() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        // Create network and host
        for request in [
            test::TestRequest::post()
                .uri("/inventory/networks")
                .set_json(serde_json::json!({"cidr":"192.168.1.0/24","description":"Test"}))
                .to_request(),
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({"name":"web.auto.org","comment":"auto test"}))
                .to_request(),
        ] {
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        // Assign IP
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/ip-addresses")
                .set_json(serde_json::json!({
                    "host_name": "web.auto.org",
                    "address": "192.168.1.10"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        // Verify A record was auto-created
        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/dns/records").to_request(),
        )
        .await;
        let body: serde_json::Value = test::read_body_json(response).await;
        let records = body["items"].as_array().expect("records list");
        let a_record = records
            .iter()
            .find(|r| r["type_name"] == "A" && r["owner_name"] == "web.auto.org");
        assert!(a_record.is_some(), "A record should be auto-created");
        assert_eq!(a_record.unwrap()["data"]["address"], "192.168.1.10");
    }
}
