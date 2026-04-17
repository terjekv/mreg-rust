use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, AuthorizationRequest, require_permission, require_permissions},
    domain::{
        filters::NetworkFilter,
        network::{CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork},
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
    services::networks as network_service,
};

use super::authz::{UpdateAuthzBuilder, request as authz_request};
use super::{
    attachment_community_assignments::AttachmentCommunityAssignmentResponse,
    attachments::{AttachmentDhcpIdentifierResponse, AttachmentPrefixReservationResponse},
    hosts::IpAddressResponse,
};

crate::page_response!(
    NetworkPageResponse,
    NetworkResponse,
    "Paginated list of networks."
);
crate::page_response!(
    ExcludedRangePageResponse,
    ExcludedRangeResponse,
    "Paginated list of excluded ranges."
);
crate::page_response!(
    UsedAddressPageResponse,
    IpAddressResponse,
    "List of used IP address assignments."
);

/// List of unused IP addresses in a network.
#[derive(Serialize, ToSchema)]
pub struct UnusedAddressListResponse {
    pub items: Vec<String>,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_networks)
        .service(create_network)
        .service(list_excluded_ranges)
        .service(create_excluded_range)
        .service(list_used_addresses)
        .service(list_unused_addresses)
        .service(get_network)
        .service(update_network)
        .service(delete_network);
}

#[derive(Deserialize)]
pub struct ListNetworksQuery {
    // Pagination + sort
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    // Special filter fields
    search: Option<String>,
    contains_ip: Option<String>,
    // Operator-based filter params
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl ListNetworksQuery {
    fn into_parts(self) -> Result<(PageRequest, NetworkFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let mut filter = NetworkFilter::from_query_params(self.filters)?;
        filter.search = self.search;
        filter.contains_ip = self.contains_ip.map(IpAddressValue::new).transpose()?;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateNetworkRequest {
    cidr: String,
    description: String,
    #[serde(default)]
    vlan: Option<u32>,
    #[serde(default)]
    dns_delegated: bool,
    #[serde(default)]
    category: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    frozen: bool,
    #[serde(default = "default_reserved")]
    reserved: u32,
}

impl CreateNetworkRequest {
    fn into_command(self) -> Result<CreateNetwork, AppError> {
        CreateNetwork::new_full(
            CidrValue::new(self.cidr)?,
            self.description,
            self.vlan,
            self.dns_delegated,
            self.category,
            self.location,
            self.frozen,
            self.reserved,
        )
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateNetworkRequest {
    description: Option<String>,
    #[schema(value_type = Option<u32>)]
    vlan: Option<Option<u32>>,
    dns_delegated: Option<bool>,
    category: Option<String>,
    location: Option<String>,
    frozen: Option<bool>,
    reserved: Option<u32>,
}

fn build_network_update_authz(
    req: &HttpRequest,
    cidr: &str,
    request: &UpdateNetworkRequest,
) -> Vec<AuthorizationRequest> {
    let mut b = UpdateAuthzBuilder::new(req, authz::actions::resource_kinds::NETWORK, cidr);
    b.field_present(
        &request.description,
        authz::actions::network::UPDATE_DESCRIPTION,
    )
    .field_clearable(
        &request.vlan,
        authz::actions::network::UPDATE_VLAN,
        "new_vlan",
        "clear_vlan",
        |v| AttrValue::Long(i64::from(*v)),
    )
    .field_bool(
        request.dns_delegated,
        authz::actions::network::UPDATE_DNS_DELEGATED,
        "new_dns_delegated",
    )
    .field_string(
        &request.category,
        authz::actions::network::UPDATE_CATEGORY,
        "new_category",
    )
    .field_string(
        &request.location,
        authz::actions::network::UPDATE_LOCATION,
        "new_location",
    )
    .field_bool(
        request.frozen,
        authz::actions::network::UPDATE_FROZEN,
        "new_frozen",
    )
    .field_u32(
        request.reserved,
        authz::actions::network::UPDATE_RESERVED,
        "new_reserved",
    );
    b.build()
}

#[derive(Deserialize)]
pub struct UnusedAddressesQuery {
    limit: Option<u32>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateExcludedRangeRequest {
    network: String,
    start_ip: String,
    end_ip: String,
    description: String,
}

impl CreateExcludedRangeRequest {
    fn into_parts(self) -> Result<(CidrValue, CreateExcludedRange), AppError> {
        Ok((
            CidrValue::new(self.network)?,
            CreateExcludedRange::new(
                IpAddressValue::new(self.start_ip)?,
                IpAddressValue::new(self.end_ip)?,
                self.description,
            )?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct NetworkResponse {
    id: Uuid,
    cidr: String,
    description: String,
    vlan: Option<u32>,
    dns_delegated: bool,
    category: String,
    location: String,
    frozen: bool,
    reserved: u32,
    capacity: NetworkCapacitySummary,
    hosts: Vec<NetworkHostInventoryResponse>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NetworkResponse {
    fn from_domain(network: &Network) -> Self {
        Self {
            id: network.id(),
            cidr: network.cidr().as_str(),
            description: network.description().to_string(),
            vlan: network.vlan(),
            dns_delegated: network.dns_delegated(),
            category: network.category().to_string(),
            location: network.location().to_string(),
            frozen: network.frozen(),
            reserved: network.reserved(),
            capacity: NetworkCapacitySummary::default(),
            hosts: Vec::new(),
            created_at: network.created_at(),
            updated_at: network.updated_at(),
        }
    }
}

#[derive(Serialize, ToSchema, Default)]
pub struct NetworkCapacitySummary {
    total_used: u32,
    total_available: u32,
}

#[derive(Serialize, ToSchema)]
pub struct NetworkHostInventoryResponse {
    host_id: Uuid,
    host_name: String,
    attachments: Vec<NetworkAttachmentInventoryResponse>,
}

#[derive(Serialize, ToSchema)]
pub struct NetworkAttachmentInventoryResponse {
    attachment_id: Uuid,
    mac_address: Option<String>,
    ip_addresses: Vec<IpAddressResponse>,
    dhcp_identifiers: Vec<AttachmentDhcpIdentifierResponse>,
    prefix_reservations: Vec<AttachmentPrefixReservationResponse>,
    community_assignments: Vec<AttachmentCommunityAssignmentResponse>,
}

async fn build_network_response(
    state: &AppState,
    network: &Network,
    include_details: bool,
) -> Result<NetworkResponse, AppError> {
    let mut response = NetworkResponse::from_domain(network);
    if !include_details {
        return Ok(response);
    }

    let attachments = state
        .storage
        .attachments()
        .list_attachments_for_network(network.cidr())
        .await?;
    let used =
        network_service::list_used_addresses(state.storage.networks(), network.cidr()).await?;
    let unused = state
        .storage
        .networks()
        .count_unused_addresses(network.cidr())
        .await?;
    let attachment_ids = attachments
        .iter()
        .map(|attachment| attachment.id())
        .collect::<Vec<_>>();
    let all_attachment_assignments = state
        .storage
        .attachment_community_assignments()
        .list_attachment_community_assignments_for_attachments(&attachment_ids)
        .await?;
    let all_dhcp_identifiers = state
        .storage
        .attachments()
        .list_attachment_dhcp_identifiers_for_attachments(&attachment_ids)
        .await?;
    let all_prefix_reservations = state
        .storage
        .attachments()
        .list_attachment_prefix_reservations_for_attachments(&attachment_ids)
        .await?;
    response.capacity = NetworkCapacitySummary {
        total_used: used.len() as u32,
        total_available: unused.min(u32::MAX as u64) as u32,
    };
    let ip_addresses_by_attachment = used.iter().fold(
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

    let mut grouped: std::collections::BTreeMap<Uuid, NetworkHostInventoryResponse> =
        std::collections::BTreeMap::new();
    for attachment in &attachments {
        grouped
            .entry(attachment.host_id())
            .or_insert_with(|| NetworkHostInventoryResponse {
                host_id: attachment.host_id(),
                host_name: attachment.host_name().as_str().to_string(),
                attachments: Vec::new(),
            })
            .attachments
            .push(NetworkAttachmentInventoryResponse {
                attachment_id: attachment.id(),
                mac_address: attachment.mac_address().map(|value| value.as_str()),
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
    response.hosts = grouped.into_values().collect();
    Ok(response)
}

#[derive(Serialize, ToSchema)]
pub struct ExcludedRangeResponse {
    id: Uuid,
    network_id: Uuid,
    start_ip: String,
    end_ip: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ExcludedRangeResponse {
    fn from_domain(range: &ExcludedRange) -> Self {
        Self {
            id: range.id(),
            network_id: range.network_id(),
            start_ip: range.start_ip().as_str(),
            end_ip: range.end_ip().as_str(),
            description: range.description().to_string(),
            created_at: range.created_at(),
            updated_at: range.updated_at(),
        }
    }
}

/// List networks with optional filters
#[utoipa::path(
    get,
    path = "/api/v1/inventory/networks",
    responses(
        (status = 200, description = "Paginated list of networks", body = NetworkPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/networks")]
pub(crate) async fn list_networks(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ListNetworksQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::network::LIST,
            authz::actions::resource_kinds::NETWORK,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = network_service::list(state.storage.networks(), &page, &filter).await?;
    let mut items = Vec::with_capacity(result.items.len());
    for network in &result.items {
        items.push(build_network_response(state.get_ref(), network, false).await?);
    }
    Ok(HttpResponse::Ok().json(PageResponse {
        items,
        total: result.total,
        next_cursor: result.next_cursor,
    }))
}

/// Create a network
#[utoipa::path(
    post,
    path = "/api/v1/inventory/networks",
    request_body = CreateNetworkRequest,
    responses(
        (status = 201, description = "Network created", body = NetworkResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Network already exists")
    ),
    tag = "Inventory"
)]
#[post("/inventory/networks")]
pub(crate) async fn create_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateNetworkRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::network::CREATE,
        authz::actions::resource_kinds::NETWORK,
        request.cidr.clone(),
    )
    .attr("cidr", AttrValue::Ip(request.cidr.clone()))
    .attr("category", AttrValue::String(request.category.clone()))
    .attr("location", AttrValue::String(request.location.clone()))
    .attr("dns_delegated", AttrValue::Bool(request.dns_delegated))
    .attr("frozen", AttrValue::Bool(request.frozen))
    .attr("reserved", AttrValue::Long(i64::from(request.reserved)));
    if let Some(vlan) = request.vlan {
        authz = authz.attr("vlan", AttrValue::Long(i64::from(vlan)));
    }
    require_permission(&state.authz, authz.build()).await?;
    let network = network_service::create(
        state.storage.networks(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created()
        .json(build_network_response(state.get_ref(), &network, false).await?))
}

/// Get a network by CIDR
#[utoipa::path(
    get,
    path = "/api/v1/inventory/networks/{cidr}",
    params(("cidr" = String, Path, description = "Network CIDR")),
    responses(
        (status = 200, description = "Network found", body = NetworkResponse),
        (status = 404, description = "Network not found")
    ),
    tag = "Inventory"
)]
#[get("/inventory/networks/{cidr:.*}")]
pub(crate) async fn get_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::network::GET,
            authz::actions::resource_kinds::NETWORK,
            cidr.as_str(),
        )
        .build(),
    )
    .await?;
    let network = network_service::get(state.storage.networks(), &cidr).await?;
    Ok(HttpResponse::Ok().json(build_network_response(state.get_ref(), &network, true).await?))
}

/// Update a network
#[utoipa::path(
    patch,
    path = "/api/v1/inventory/networks/{cidr}",
    params(("cidr" = String, Path, description = "Network CIDR")),
    request_body = UpdateNetworkRequest,
    responses(
        (status = 200, description = "Network updated", body = NetworkResponse),
        (status = 404, description = "Network not found")
    ),
    tag = "Inventory"
)]
#[patch("/inventory/networks/{cidr:.*}")]
pub(crate) async fn update_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateNetworkRequest>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(path.into_inner())?;
    let request = payload.into_inner();
    let authz_requests = build_network_update_authz(&req, &cidr.as_str(), &request);
    require_permissions(&state.authz, authz_requests).await?;
    let command = UpdateNetwork {
        description: request.description,
        vlan: request.vlan,
        dns_delegated: request.dns_delegated,
        category: request.category,
        location: request.location,
        frozen: request.frozen,
        reserved: request.reserved,
    };
    let network = network_service::update(
        state.storage.networks(),
        state.storage.audit(),
        &state.events,
        &cidr,
        command,
    )
    .await?;
    Ok(HttpResponse::Ok().json(build_network_response(state.get_ref(), &network, false).await?))
}

/// List used addresses in a network
#[utoipa::path(
    get,
    path = "/api/v1/inventory/networks/{cidr}/used_addresses",
    params(("cidr" = String, Path, description = "Network CIDR")),
    responses(
        (status = 200, description = "List of used IP addresses in the network", body = UsedAddressPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/networks/{cidr:.*}/used_addresses")]
pub(crate) async fn list_used_addresses(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::network::ADDRESS_LIST_USED,
            authz::actions::resource_kinds::NETWORK,
            cidr.as_str(),
        )
        .build(),
    )
    .await?;
    let assignments = network_service::list_used_addresses(state.storage.networks(), &cidr).await?;
    let items: Vec<_> = assignments
        .iter()
        .map(IpAddressResponse::from_domain)
        .collect();
    Ok(HttpResponse::Ok().json(serde_json::json!({ "items": items })))
}

/// List unused addresses in a network
#[utoipa::path(
    get,
    path = "/api/v1/inventory/networks/{cidr}/unused_addresses",
    params(("cidr" = String, Path, description = "Network CIDR")),
    responses(
        (status = 200, description = "List of unused IP addresses in the network", body = UnusedAddressListResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/networks/{cidr:.*}/unused_addresses")]
pub(crate) async fn list_unused_addresses(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<UnusedAddressesQuery>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(path.into_inner())?;
    let mut authz = authz_request(
        &req,
        authz::actions::network::ADDRESS_LIST_UNUSED,
        authz::actions::resource_kinds::NETWORK,
        cidr.as_str(),
    );
    if let Some(limit) = query.limit {
        authz = authz.attr("limit", AttrValue::Long(i64::from(limit)));
    }
    require_permission(&state.authz, authz.build()).await?;
    let addresses =
        network_service::list_unused_addresses(state.storage.networks(), &cidr, query.limit)
            .await?;
    let items: Vec<String> = addresses.iter().map(|a| a.as_str()).collect();
    Ok(HttpResponse::Ok().json(UnusedAddressListResponse { items }))
}

/// Delete a network
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/networks/{cidr}",
    params(("cidr" = String, Path, description = "Network CIDR")),
    responses(
        (status = 204, description = "Network deleted"),
        (status = 404, description = "Network not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/networks/{cidr:.*}")]
pub(crate) async fn delete_network(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::network::DELETE,
            authz::actions::resource_kinds::NETWORK,
            cidr.as_str(),
        )
        .build(),
    )
    .await?;
    network_service::delete(
        state.storage.networks(),
        state.storage.audit(),
        &state.events,
        &cidr,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// List excluded ranges for a network
#[utoipa::path(
    get,
    path = "/api/v1/inventory/networks/{cidr}/excluded-ranges",
    params(("cidr" = String, Path, description = "Network CIDR")),
    responses(
        (status = 200, description = "List of excluded ranges", body = ExcludedRangePageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/networks/{cidr:.*}/excluded-ranges")]
pub(crate) async fn list_excluded_ranges(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let cidr = CidrValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::network::EXCLUDED_RANGE_LIST,
            authz::actions::resource_kinds::NETWORK,
            cidr.as_str(),
        )
        .build(),
    )
    .await?;
    let page =
        network_service::list_excluded_ranges(state.storage.networks(), &cidr, &PageRequest::all())
            .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        ExcludedRangeResponse::from_domain,
    )))
}

/// Create an excluded range for a network
#[utoipa::path(
    post,
    path = "/api/v1/inventory/networks/excluded-ranges",
    request_body = CreateExcludedRangeRequest,
    responses(
        (status = 201, description = "Excluded range created", body = ExcludedRangeResponse),
        (status = 400, description = "Validation error")
    ),
    tag = "Inventory"
)]
#[post("/inventory/networks/excluded-ranges")]
pub(crate) async fn create_excluded_range(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateExcludedRangeRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::network::EXCLUDED_RANGE_CREATE,
            authz::actions::resource_kinds::EXCLUDED_RANGE,
            format!(
                "{}:{}-{}",
                request.network, request.start_ip, request.end_ip
            ),
        )
        .attr("network", AttrValue::Ip(request.network.clone()))
        .attr("start_ip", AttrValue::Ip(request.start_ip.clone()))
        .attr("end_ip", AttrValue::Ip(request.end_ip.clone()))
        .attr(
            "description",
            AttrValue::String(request.description.clone()),
        )
        .build(),
    )
    .await?;
    let (network, command) = request.into_parts()?;
    let range = network_service::add_excluded_range(
        state.storage.networks(),
        state.storage.audit(),
        &state.events,
        &network,
        command,
    )
    .await?;
    Ok(HttpResponse::Created().json(ExcludedRangeResponse::from_domain(&range)))
}

fn default_reserved() -> u32 {
    3
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_network_and_excluded_range() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let create_network = test::TestRequest::post()
            .uri("/inventory/networks")
            .set_json(serde_json::json!({
                "cidr": "10.0.0.0/24",
                "description": "Production LAN",
                "reserved": 5
            }))
            .to_request();
        let response = test::call_service(&app, create_network).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let create_range = test::TestRequest::post()
            .uri("/inventory/networks/excluded-ranges")
            .set_json(serde_json::json!({
                "network": "10.0.0.0/24",
                "start_ip": "10.0.0.10",
                "end_ip": "10.0.0.20",
                "description": "Reserved devices"
            }))
            .to_request();
        let response = test::call_service(&app, create_range).await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[actix_web::test]
    async fn list_networks_supports_contains_ip_filter() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        for request in [
            serde_json::json!({"cidr":"10.0.0.0/24","description":"LAN","reserved":3}),
            serde_json::json!({"cidr":"10.1.0.0/24","description":"Other","reserved":3}),
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

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/inventory/networks?contains_ip=10.0.0.42")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
        assert_eq!(body["items"][0]["cidr"], "10.0.0.0/24");
    }
}
