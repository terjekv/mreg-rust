use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, AuthorizationRequest, require_permission, require_permissions},
    domain::{
        pagination::{PageRequest, PageResponse},
        types::{CidrValue, DnsName, EmailAddressValue, SerialNumber, Ttl, ZoneName},
        zone::{CreateReverseZone, ReverseZone, UpdateReverseZone},
    },
    errors::AppError,
    services::zones as zone_service,
};

use crate::api::v1::authz::{UpdateAuthzBuilder, request as authz_request, string_set};

use super::{default_expire, default_refresh, default_retry, default_serial_no, default_ttl_value};

crate::page_response!(
    ReverseZonePageResponse,
    ReverseZoneResponse,
    "Paginated list of reverse zones."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_reverse_zones)
        .service(create_reverse_zone)
        .service(get_reverse_zone)
        .service(update_reverse_zone)
        .service(delete_reverse_zone);
}

#[derive(Deserialize, ToSchema)]
pub struct CreateReverseZoneRequest {
    name: String,
    network: Option<String>,
    primary_ns: String,
    #[serde(default)]
    nameservers: Vec<String>,
    email: String,
    #[serde(default = "default_serial_no")]
    serial_no: u64,
    #[serde(default = "default_refresh")]
    refresh: u32,
    #[serde(default = "default_retry")]
    retry: u32,
    #[serde(default = "default_expire")]
    expire: u32,
    #[serde(default = "default_ttl_value")]
    soa_ttl: u32,
    #[serde(default = "default_ttl_value")]
    default_ttl: u32,
}

impl CreateReverseZoneRequest {
    fn into_command(self) -> Result<CreateReverseZone, AppError> {
        let nameservers = self
            .nameservers
            .into_iter()
            .map(DnsName::new)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CreateReverseZone::new(
            ZoneName::new(self.name)?,
            self.network.map(CidrValue::new).transpose()?,
            DnsName::new(self.primary_ns)?,
            nameservers,
            EmailAddressValue::new(self.email)?,
            SerialNumber::new(self.serial_no)?,
            self.refresh,
            self.retry,
            self.expire,
            Ttl::new(self.soa_ttl)?,
            Ttl::new(self.default_ttl)?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct ReverseZoneResponse {
    id: Uuid,
    name: String,
    network: Option<String>,
    updated: bool,
    primary_ns: String,
    nameservers: Vec<String>,
    email: String,
    serial_no: u64,
    serial_no_updated_at: DateTime<Utc>,
    refresh: u32,
    retry: u32,
    expire: u32,
    soa_ttl: u32,
    default_ttl: u32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ReverseZoneResponse {
    pub(crate) fn from_domain(zone: &ReverseZone) -> Self {
        Self {
            id: zone.id(),
            name: zone.name().as_str().to_string(),
            network: zone.network().map(|network| network.as_str()),
            updated: zone.updated(),
            primary_ns: zone.primary_ns().as_str().to_string(),
            nameservers: zone
                .nameservers()
                .iter()
                .map(|nameserver| nameserver.as_str().to_string())
                .collect(),
            email: zone.email().as_str().to_string(),
            serial_no: zone.serial_no().as_u64(),
            serial_no_updated_at: zone.serial_no_updated_at(),
            refresh: zone.refresh(),
            retry: zone.retry(),
            expire: zone.expire(),
            soa_ttl: zone.soa_ttl().as_u32(),
            default_ttl: zone.default_ttl().as_u32(),
            created_at: zone.created_at(),
            updated_at: zone.updated_at(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateReverseZoneRequest {
    primary_ns: Option<String>,
    nameservers: Option<Vec<String>>,
    email: Option<String>,
    refresh: Option<u32>,
    retry: Option<u32>,
    expire: Option<u32>,
    soa_ttl: Option<u32>,
    default_ttl: Option<u32>,
}

fn build_reverse_zone_update_authz(
    req: &HttpRequest,
    name: &str,
    request: &UpdateReverseZoneRequest,
) -> Vec<AuthorizationRequest> {
    let mut b = UpdateAuthzBuilder::new(req, authz::actions::resource_kinds::REVERSE_ZONE, name);
    b.field_string(
        &request.primary_ns,
        authz::actions::zone::reverse::UPDATE_PRIMARY_NS,
        "new_primary_ns",
    )
    .field_string_set(
        &request.nameservers,
        authz::actions::zone::reverse::UPDATE_NAMESERVERS,
        "new_nameservers",
    )
    .field_string(
        &request.email,
        authz::actions::zone::reverse::UPDATE_EMAIL,
        "new_email",
    )
    .timing_fields(
        authz::actions::zone::reverse::UPDATE_TIMING,
        &[
            ("refresh", request.refresh),
            ("retry", request.retry),
            ("expire", request.expire),
            ("soa_ttl", request.soa_ttl),
            ("default_ttl", request.default_ttl),
        ],
    );
    b.build()
}

/// List all reverse zones
#[utoipa::path(
    get,
    path = "/api/v1/dns/reverse-zones",
    params(PageRequest),
    responses(
        (status = 200, description = "Paginated list of reverse zones", body = ReverseZonePageResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/reverse-zones")]
pub(crate) async fn list_reverse_zones(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::reverse::LIST,
            authz::actions::resource_kinds::REVERSE_ZONE,
            "*",
        )
        .build(),
    )
    .await?;
    let page = zone_service::list_reverse(state.storage.zones(), &query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        ReverseZoneResponse::from_domain,
    )))
}

/// Create a reverse zone
#[utoipa::path(
    post,
    path = "/api/v1/dns/reverse-zones",
    request_body = CreateReverseZoneRequest,
    responses(
        (status = 201, description = "Reverse zone created", body = ReverseZoneResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Zone already exists")
    ),
    tag = "DNS"
)]
#[post("/dns/reverse-zones")]
pub(crate) async fn create_reverse_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateReverseZoneRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::zone::reverse::CREATE,
        authz::actions::resource_kinds::REVERSE_ZONE,
        request.name.clone(),
    )
    .attr("name", AttrValue::String(request.name.clone()))
    .attr("primary_ns", AttrValue::String(request.primary_ns.clone()))
    .attr("nameservers", string_set(request.nameservers.clone()))
    .attr("email", AttrValue::String(request.email.clone()))
    .attr("serial_no", AttrValue::Long(request.serial_no as i64))
    .attr("refresh", AttrValue::Long(i64::from(request.refresh)))
    .attr("retry", AttrValue::Long(i64::from(request.retry)))
    .attr("expire", AttrValue::Long(i64::from(request.expire)))
    .attr("soa_ttl", AttrValue::Long(i64::from(request.soa_ttl)))
    .attr(
        "default_ttl",
        AttrValue::Long(i64::from(request.default_ttl)),
    );
    if let Some(network) = &request.network {
        authz = authz.attr("network", AttrValue::Ip(network.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let zone = zone_service::create_reverse(
        state.storage.zones(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created().json(ReverseZoneResponse::from_domain(&zone)))
}

/// Get a reverse zone by name
#[utoipa::path(
    get,
    path = "/api/v1/dns/reverse-zones/{name}",
    params(("name" = String, Path, description = "Zone name")),
    responses(
        (status = 200, description = "Reverse zone found", body = ReverseZoneResponse),
        (status = 404, description = "Reverse zone not found")
    ),
    tag = "DNS"
)]
#[get("/dns/reverse-zones/{name}")]
pub(crate) async fn get_reverse_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::reverse::GET,
            authz::actions::resource_kinds::REVERSE_ZONE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let zone = zone_service::get_reverse(state.storage.zones(), &name).await?;
    Ok(HttpResponse::Ok().json(ReverseZoneResponse::from_domain(&zone)))
}

/// Update a reverse zone
#[utoipa::path(
    patch,
    path = "/api/v1/dns/reverse-zones/{name}",
    params(("name" = String, Path, description = "Zone name")),
    request_body = UpdateReverseZoneRequest,
    responses(
        (status = 200, description = "Reverse zone updated", body = ReverseZoneResponse),
        (status = 404, description = "Reverse zone not found")
    ),
    tag = "DNS"
)]
#[patch("/dns/reverse-zones/{name}")]
pub(crate) async fn update_reverse_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateReverseZoneRequest>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(path.into_inner())?;
    let request = payload.into_inner();
    let authz_requests = build_reverse_zone_update_authz(&req, name.as_str(), &request);
    require_permissions(&state.authz, authz_requests).await?;
    let primary_ns = request.primary_ns.map(DnsName::new).transpose()?;
    let nameservers = request
        .nameservers
        .map(|ns| {
            ns.into_iter()
                .map(DnsName::new)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let email = request.email.map(EmailAddressValue::new).transpose()?;
    let soa_ttl = request.soa_ttl.map(Ttl::new).transpose()?;
    let default_ttl = request.default_ttl.map(Ttl::new).transpose()?;
    let command = UpdateReverseZone {
        primary_ns,
        nameservers,
        email,
        refresh: request.refresh,
        retry: request.retry,
        expire: request.expire,
        soa_ttl,
        default_ttl,
    };
    let zone = zone_service::update_reverse(
        state.storage.zones(),
        state.storage.audit(),
        &state.events,
        &name,
        command,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ReverseZoneResponse::from_domain(&zone)))
}

/// Delete a reverse zone
#[utoipa::path(
    delete,
    path = "/api/v1/dns/reverse-zones/{name}",
    params(("name" = String, Path, description = "Zone name")),
    responses(
        (status = 204, description = "Reverse zone deleted"),
        (status = 404, description = "Reverse zone not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/reverse-zones/{name}")]
pub(crate) async fn delete_reverse_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::reverse::DELETE,
            authz::actions::resource_kinds::REVERSE_ZONE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    zone_service::delete_reverse(
        state.storage.zones(),
        state.storage.audit(),
        &state.events,
        &name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
