use actix_web::{HttpRequest, HttpResponse, delete, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        pagination::{PageRequest, PageResponse},
        types::{DnsName, ZoneName},
        zone::{
            CreateForwardZoneDelegation, CreateReverseZoneDelegation, ForwardZoneDelegation,
            ReverseZoneDelegation,
        },
    },
    errors::AppError,
};

use crate::api::v1::authz::{request as authz_request, string_set};

crate::page_response!(
    ForwardZoneDelegationPageResponse,
    ForwardZoneDelegationResponse,
    "Paginated list of forward zone delegations."
);
crate::page_response!(
    ReverseZoneDelegationPageResponse,
    ReverseZoneDelegationResponse,
    "Paginated list of reverse zone delegations."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_forward_zone_delegations)
        .service(create_forward_zone_delegation)
        .service(delete_forward_zone_delegation)
        .service(list_reverse_zone_delegations)
        .service(create_reverse_zone_delegation)
        .service(delete_reverse_zone_delegation);
}

#[derive(Deserialize, ToSchema)]
pub struct CreateDelegationRequest {
    name: String,
    #[serde(default)]
    comment: String,
    #[serde(default)]
    nameservers: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ForwardZoneDelegationResponse {
    id: Uuid,
    zone_id: Uuid,
    name: String,
    comment: String,
    nameservers: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ForwardZoneDelegationResponse {
    fn from_domain(d: &ForwardZoneDelegation) -> Self {
        Self {
            id: d.id(),
            zone_id: d.zone_id(),
            name: d.name().as_str().to_string(),
            comment: d.comment().to_string(),
            nameservers: d
                .nameservers()
                .iter()
                .map(|ns| ns.as_str().to_string())
                .collect(),
            created_at: d.created_at(),
            updated_at: d.updated_at(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ReverseZoneDelegationResponse {
    id: Uuid,
    zone_id: Uuid,
    name: String,
    comment: String,
    nameservers: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ReverseZoneDelegationResponse {
    fn from_domain(d: &ReverseZoneDelegation) -> Self {
        Self {
            id: d.id(),
            zone_id: d.zone_id(),
            name: d.name().as_str().to_string(),
            comment: d.comment().to_string(),
            nameservers: d
                .nameservers()
                .iter()
                .map(|ns| ns.as_str().to_string())
                .collect(),
            created_at: d.created_at(),
            updated_at: d.updated_at(),
        }
    }
}

// --- Forward zone delegation endpoints ---

/// List delegations for a forward zone
#[utoipa::path(
    get,
    path = "/api/v1/dns/forward-zones/{zone_name}/delegations",
    params(
        ("zone_name" = String, Path, description = "Forward zone name"),
        PageRequest,
    ),
    responses(
        (status = 200, description = "Paginated list of forward zone delegations", body = ForwardZoneDelegationPageResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/forward-zones/{zone_name}/delegations")]
pub(crate) async fn list_forward_zone_delegations(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    let zone_name = ZoneName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::delegation::LIST,
            authz::actions::resource_kinds::FORWARD_ZONE,
            zone_name.as_str(),
        )
        .build(),
    )
    .await?;
    let page = state
        .services
        .zones()
        .list_forward_delegations(&zone_name, &query.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        ForwardZoneDelegationResponse::from_domain,
    )))
}

/// Create a delegation for a forward zone
#[utoipa::path(
    post,
    path = "/api/v1/dns/forward-zones/{zone_name}/delegations",
    params(("zone_name" = String, Path, description = "Forward zone name")),
    request_body = CreateDelegationRequest,
    responses(
        (status = 201, description = "Delegation created", body = ForwardZoneDelegationResponse),
        (status = 400, description = "Validation error")
    ),
    tag = "DNS"
)]
#[post("/dns/forward-zones/{zone_name}/delegations")]
pub(crate) async fn create_forward_zone_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<CreateDelegationRequest>,
) -> Result<HttpResponse, AppError> {
    let zone_name = ZoneName::new(path.into_inner())?;
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::delegation::CREATE,
            authz::actions::resource_kinds::FORWARD_ZONE_DELEGATION,
            format!("{}:{}", zone_name.as_str(), request.name),
        )
        .attr(
            "zone_name",
            AttrValue::String(zone_name.as_str().to_string()),
        )
        .attr("comment", AttrValue::String(request.comment.clone()))
        .attr("nameservers", string_set(request.nameservers.clone()))
        .build(),
    )
    .await?;
    let nameservers = request
        .nameservers
        .into_iter()
        .map(DnsName::new)
        .collect::<Result<Vec<_>, _>>()?;
    let command = CreateForwardZoneDelegation::new(
        zone_name,
        DnsName::new(request.name)?,
        request.comment,
        nameservers,
    );
    let delegation = state
        .services
        .zones()
        .create_forward_delegation(command)
        .await?;

    Ok(HttpResponse::Created().json(ForwardZoneDelegationResponse::from_domain(&delegation)))
}

/// Delete a forward zone delegation
#[utoipa::path(
    delete,
    path = "/api/v1/dns/forward-zone-delegations/{id}",
    params(("id" = Uuid, Path, description = "Delegation ID")),
    responses(
        (status = 204, description = "Delegation deleted"),
        (status = 404, description = "Delegation not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/forward-zone-delegations/{id}")]
pub(crate) async fn delete_forward_zone_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let delegation_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::delegation::DELETE,
            authz::actions::resource_kinds::FORWARD_ZONE_DELEGATION,
            delegation_id.to_string(),
        )
        .build(),
    )
    .await?;
    state
        .services
        .zones()
        .delete_forward_delegation(delegation_id)
        .await?;

    Ok(HttpResponse::NoContent().finish())
}

// --- Reverse zone delegation endpoints ---

/// List delegations for a reverse zone
#[utoipa::path(
    get,
    path = "/api/v1/dns/reverse-zones/{zone_name}/delegations",
    params(
        ("zone_name" = String, Path, description = "Reverse zone name"),
        PageRequest,
    ),
    responses(
        (status = 200, description = "Paginated list of reverse zone delegations", body = ReverseZoneDelegationPageResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/reverse-zones/{zone_name}/delegations")]
pub(crate) async fn list_reverse_zone_delegations(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    let zone_name = ZoneName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::reverse::delegation::LIST,
            authz::actions::resource_kinds::REVERSE_ZONE,
            zone_name.as_str(),
        )
        .build(),
    )
    .await?;
    let page = state
        .services
        .zones()
        .list_reverse_delegations(&zone_name, &query.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        ReverseZoneDelegationResponse::from_domain,
    )))
}

/// Create a delegation for a reverse zone
#[utoipa::path(
    post,
    path = "/api/v1/dns/reverse-zones/{zone_name}/delegations",
    params(("zone_name" = String, Path, description = "Reverse zone name")),
    request_body = CreateDelegationRequest,
    responses(
        (status = 201, description = "Delegation created", body = ReverseZoneDelegationResponse),
        (status = 400, description = "Validation error")
    ),
    tag = "DNS"
)]
#[post("/dns/reverse-zones/{zone_name}/delegations")]
pub(crate) async fn create_reverse_zone_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<CreateDelegationRequest>,
) -> Result<HttpResponse, AppError> {
    let zone_name = ZoneName::new(path.into_inner())?;
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::reverse::delegation::CREATE,
            authz::actions::resource_kinds::REVERSE_ZONE_DELEGATION,
            format!("{}:{}", zone_name.as_str(), request.name),
        )
        .attr(
            "zone_name",
            AttrValue::String(zone_name.as_str().to_string()),
        )
        .attr("comment", AttrValue::String(request.comment.clone()))
        .attr("nameservers", string_set(request.nameservers.clone()))
        .build(),
    )
    .await?;
    let nameservers = request
        .nameservers
        .into_iter()
        .map(DnsName::new)
        .collect::<Result<Vec<_>, _>>()?;
    let command = CreateReverseZoneDelegation::new(
        zone_name,
        DnsName::new(request.name)?,
        request.comment,
        nameservers,
    );
    let delegation = state
        .services
        .zones()
        .create_reverse_delegation(command)
        .await?;

    Ok(HttpResponse::Created().json(ReverseZoneDelegationResponse::from_domain(&delegation)))
}

/// Delete a reverse zone delegation
#[utoipa::path(
    delete,
    path = "/api/v1/dns/reverse-zone-delegations/{id}",
    params(("id" = Uuid, Path, description = "Delegation ID")),
    responses(
        (status = 204, description = "Delegation deleted"),
        (status = 404, description = "Delegation not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/reverse-zone-delegations/{id}")]
pub(crate) async fn delete_reverse_zone_delegation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let delegation_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::reverse::delegation::DELETE,
            authz::actions::resource_kinds::REVERSE_ZONE_DELEGATION,
            delegation_id.to_string(),
        )
        .build(),
    )
    .await?;
    state
        .services
        .zones()
        .delete_reverse_delegation(delegation_id)
        .await?;

    Ok(HttpResponse::NoContent().finish())
}
