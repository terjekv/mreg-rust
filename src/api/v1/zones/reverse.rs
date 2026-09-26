use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, AuthorizationRequest},
    domain::{
        pagination::{PageRequest, PageResponse},
        types::{CidrValue, DnsName, EmailAddressValue, SerialNumber, SoaSeconds, Ttl, ZoneName},
        zone::{CreateReverseZone, ReverseZone, UpdateReverseZone},
    },
    errors::AppError,
};

use crate::api::v1::authz::{
    UpdateAuthzBuilder, request as authz_request, require, require_all, string_set,
};

use super::{
    default_expire, default_negative_ttl, default_refresh, default_retry, default_serial_no,
    default_ttl_value,
};

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
    #[schema(value_type = String)]
    name: ZoneName,
    #[schema(value_type = Option<String>)]
    network: Option<CidrValue>,
    #[schema(value_type = String)]
    primary_ns: DnsName,
    #[serde(default)]
    #[schema(value_type = Vec<String>)]
    nameservers: Vec<DnsName>,
    #[schema(value_type = String)]
    email: EmailAddressValue,
    #[serde(default = "default_serial_no")]
    #[schema(value_type = u32)]
    serial_no: SerialNumber,
    #[serde(default = "default_refresh")]
    #[schema(value_type = u32)]
    refresh: SoaSeconds,
    #[serde(default = "default_retry")]
    #[schema(value_type = u32)]
    retry: SoaSeconds,
    #[serde(default = "default_expire")]
    #[schema(value_type = u32)]
    expire: SoaSeconds,
    #[serde(default = "default_ttl_value")]
    #[schema(value_type = u32)]
    soa_record_ttl: Ttl,
    #[serde(default = "default_negative_ttl")]
    #[schema(value_type = u32)]
    negative_ttl: Ttl,
    #[serde(default = "default_ttl_value")]
    #[schema(value_type = u32)]
    default_ttl: Ttl,
}

impl CreateReverseZoneRequest {
    fn into_command(self) -> Result<CreateReverseZone, AppError> {
        let nameservers = self.nameservers;

        CreateReverseZone::new(
            self.name,
            self.network,
            self.primary_ns,
            nameservers,
            self.email,
            self.serial_no,
            self.refresh,
            self.retry,
            self.expire,
            self.soa_record_ttl,
            self.negative_ttl,
            self.default_ttl,
        )
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
    serial_no: u32,
    serial_no_updated_at: DateTime<Utc>,
    refresh: u32,
    retry: u32,
    expire: u32,
    soa_record_ttl: u32,
    negative_ttl: u32,
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
            serial_no: zone.serial_no().as_u32(),
            serial_no_updated_at: zone.serial_no_updated_at(),
            refresh: zone.refresh().as_u32(),
            retry: zone.retry().as_u32(),
            expire: zone.expire().as_u32(),
            soa_record_ttl: zone.soa_record_ttl().as_u32(),
            negative_ttl: zone.negative_ttl().as_u32(),
            default_ttl: zone.default_ttl().as_u32(),
            created_at: zone.created_at(),
            updated_at: zone.updated_at(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateReverseZoneRequest {
    #[schema(value_type = Option<String>)]
    primary_ns: Option<DnsName>,
    #[schema(value_type = Option<Vec<String>>)]
    nameservers: Option<Vec<DnsName>>,
    #[schema(value_type = Option<String>)]
    email: Option<EmailAddressValue>,
    #[schema(value_type = Option<u32>)]
    refresh: Option<SoaSeconds>,
    #[schema(value_type = Option<u32>)]
    retry: Option<SoaSeconds>,
    #[schema(value_type = Option<u32>)]
    expire: Option<SoaSeconds>,
    #[schema(value_type = Option<u32>)]
    soa_record_ttl: Option<Ttl>,
    #[schema(value_type = Option<u32>)]
    negative_ttl: Option<Ttl>,
    #[schema(value_type = Option<u32>)]
    default_ttl: Option<Ttl>,
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
            ("refresh", request.refresh.map(|value| value.as_u32())),
            ("retry", request.retry.map(|value| value.as_u32())),
            ("expire", request.expire.map(|value| value.as_u32())),
            (
                "soa_record_ttl",
                request.soa_record_ttl.map(|value| value.as_u32()),
            ),
            (
                "negative_ttl",
                request.negative_ttl.map(|value| value.as_u32()),
            ),
            (
                "default_ttl",
                request.default_ttl.map(|value| value.as_u32()),
            ),
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
    require(
        &state,
        authz_request(
            &req,
            authz::actions::zone::reverse::LIST,
            authz::actions::resource_kinds::REVERSE_ZONE,
            "*",
        ),
    )
    .await?;
    let page = state
        .services
        .zones()
        .list_reverse(&query.into_inner())
        .await?;
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
        &request.name,
    )
    .attr("name", AttrValue::String(request.name.to_string()))
    .attr(
        "primary_ns",
        AttrValue::String(request.primary_ns.to_string()),
    )
    .attr("nameservers", string_set(&request.nameservers))
    .attr("email", AttrValue::String(request.email.to_string()))
    .attr("serial_no", AttrValue::Long(request.serial_no.as_i64()))
    .attr(
        "refresh",
        AttrValue::Long(i64::from(request.refresh.as_u32())),
    )
    .attr("retry", AttrValue::Long(i64::from(request.retry.as_u32())))
    .attr(
        "expire",
        AttrValue::Long(i64::from(request.expire.as_u32())),
    )
    .attr(
        "soa_record_ttl",
        AttrValue::Long(i64::from(request.soa_record_ttl.as_u32())),
    )
    .attr(
        "negative_ttl",
        AttrValue::Long(i64::from(request.negative_ttl.as_u32())),
    )
    .attr(
        "default_ttl",
        AttrValue::Long(i64::from(request.default_ttl.as_u32())),
    );
    if let Some(network) = &request.network {
        authz = authz.attr("network", AttrValue::Ip(network.to_string()));
    }
    require(&state, authz).await?;
    let zone = state
        .services
        .zones()
        .create_reverse(request.into_command()?)
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
    path: web::Path<ZoneName>,
) -> Result<HttpResponse, AppError> {
    let name = path.into_inner();
    require(
        &state,
        authz_request(
            &req,
            authz::actions::zone::reverse::GET,
            authz::actions::resource_kinds::REVERSE_ZONE,
            name.as_str(),
        ),
    )
    .await?;
    let zone = state.services.zones().get_reverse(&name).await?;
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
    path: web::Path<ZoneName>,
    payload: web::Json<UpdateReverseZoneRequest>,
) -> Result<HttpResponse, AppError> {
    let name = path.into_inner();
    let request = payload.into_inner();
    let authz_requests = build_reverse_zone_update_authz(&req, name.as_str(), &request);
    require_all(&state, authz_requests).await?;
    let primary_ns = request.primary_ns;
    let nameservers = request.nameservers;
    let email = request.email;
    let refresh = request.refresh;
    let retry = request.retry;
    let expire = request.expire;
    let soa_record_ttl = request.soa_record_ttl;
    let negative_ttl = request.negative_ttl;
    let default_ttl = request.default_ttl;
    let command = UpdateReverseZone {
        primary_ns,
        nameservers,
        email,
        refresh,
        retry,
        expire,
        soa_record_ttl,
        negative_ttl,
        default_ttl,
    };
    let zone = state
        .services
        .zones()
        .update_reverse(&name, command)
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
    path: web::Path<ZoneName>,
) -> Result<HttpResponse, AppError> {
    let name = path.into_inner();
    require(
        &state,
        authz_request(
            &req,
            authz::actions::zone::reverse::DELETE,
            authz::actions::resource_kinds::REVERSE_ZONE,
            name.as_str(),
        ),
    )
    .await?;
    state.services.zones().delete_reverse(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}
