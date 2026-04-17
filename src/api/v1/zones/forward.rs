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
        types::{DnsName, EmailAddressValue, SerialNumber, SoaSeconds, Ttl, ZoneName},
        zone::{CreateForwardZone, ForwardZone, UpdateForwardZone},
    },
    errors::AppError,
};

use crate::api::v1::authz::{UpdateAuthzBuilder, request as authz_request, string_set};

use super::{default_expire, default_refresh, default_retry, default_serial_no, default_ttl_value};

crate::page_response!(
    ForwardZonePageResponse,
    ForwardZoneResponse,
    "Paginated list of forward zones."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_forward_zones)
        .service(create_forward_zone)
        .service(get_forward_zone)
        .service(update_forward_zone)
        .service(delete_forward_zone);
}

#[derive(Deserialize, ToSchema)]
pub struct CreateForwardZoneRequest {
    name: String,
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

impl CreateForwardZoneRequest {
    fn into_command(self) -> Result<CreateForwardZone, AppError> {
        let nameservers = self
            .nameservers
            .into_iter()
            .map(DnsName::new)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CreateForwardZone::new(
            ZoneName::new(self.name)?,
            DnsName::new(self.primary_ns)?,
            nameservers,
            EmailAddressValue::new(self.email)?,
            SerialNumber::new(self.serial_no)?,
            SoaSeconds::new(self.refresh)?,
            SoaSeconds::new(self.retry)?,
            SoaSeconds::new(self.expire)?,
            Ttl::new(self.soa_ttl)?,
            Ttl::new(self.default_ttl)?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct ForwardZoneResponse {
    id: Uuid,
    name: String,
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

impl ForwardZoneResponse {
    pub(crate) fn from_domain(zone: &ForwardZone) -> Self {
        Self {
            id: zone.id(),
            name: zone.name().as_str().to_string(),
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
            refresh: zone.refresh().as_u32(),
            retry: zone.retry().as_u32(),
            expire: zone.expire().as_u32(),
            soa_ttl: zone.soa_ttl().as_u32(),
            default_ttl: zone.default_ttl().as_u32(),
            created_at: zone.created_at(),
            updated_at: zone.updated_at(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateForwardZoneRequest {
    primary_ns: Option<String>,
    nameservers: Option<Vec<String>>,
    email: Option<String>,
    refresh: Option<u32>,
    retry: Option<u32>,
    expire: Option<u32>,
    soa_ttl: Option<u32>,
    default_ttl: Option<u32>,
}

fn build_forward_zone_update_authz(
    req: &HttpRequest,
    name: &str,
    request: &UpdateForwardZoneRequest,
) -> Vec<AuthorizationRequest> {
    let mut b = UpdateAuthzBuilder::new(req, authz::actions::resource_kinds::FORWARD_ZONE, name);
    b.field_string(
        &request.primary_ns,
        authz::actions::zone::forward::UPDATE_PRIMARY_NS,
        "new_primary_ns",
    )
    .field_string_set(
        &request.nameservers,
        authz::actions::zone::forward::UPDATE_NAMESERVERS,
        "new_nameservers",
    )
    .field_string(
        &request.email,
        authz::actions::zone::forward::UPDATE_EMAIL,
        "new_email",
    )
    .timing_fields(
        authz::actions::zone::forward::UPDATE_TIMING,
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

/// List all forward zones
#[utoipa::path(
    get,
    path = "/api/v1/dns/forward-zones",
    params(PageRequest),
    responses(
        (status = 200, description = "Paginated list of forward zones", body = ForwardZonePageResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/forward-zones")]
pub(crate) async fn list_forward_zones(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::LIST,
            authz::actions::resource_kinds::FORWARD_ZONE,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .services
        .zones()
        .list_forward(&query.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        ForwardZoneResponse::from_domain,
    )))
}

/// Create a forward zone
#[utoipa::path(
    post,
    path = "/api/v1/dns/forward-zones",
    request_body = CreateForwardZoneRequest,
    responses(
        (status = 201, description = "Forward zone created", body = ForwardZoneResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Zone already exists")
    ),
    tag = "DNS"
)]
#[post("/dns/forward-zones")]
pub(crate) async fn create_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateForwardZoneRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::CREATE,
            authz::actions::resource_kinds::FORWARD_ZONE,
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
        )
        .build(),
    )
    .await?;
    let zone = state
        .services
        .zones()
        .create_forward(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(ForwardZoneResponse::from_domain(&zone)))
}

/// Get a forward zone by name
#[utoipa::path(
    get,
    path = "/api/v1/dns/forward-zones/{name}",
    params(("name" = String, Path, description = "Zone name")),
    responses(
        (status = 200, description = "Forward zone found", body = ForwardZoneResponse),
        (status = 404, description = "Forward zone not found")
    ),
    tag = "DNS"
)]
#[get("/dns/forward-zones/{name}")]
pub(crate) async fn get_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::GET,
            authz::actions::resource_kinds::FORWARD_ZONE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let zone = state.services.zones().get_forward(&name).await?;
    Ok(HttpResponse::Ok().json(ForwardZoneResponse::from_domain(&zone)))
}

/// Update a forward zone
#[utoipa::path(
    patch,
    path = "/api/v1/dns/forward-zones/{name}",
    params(("name" = String, Path, description = "Zone name")),
    request_body = UpdateForwardZoneRequest,
    responses(
        (status = 200, description = "Forward zone updated", body = ForwardZoneResponse),
        (status = 404, description = "Forward zone not found")
    ),
    tag = "DNS"
)]
#[patch("/dns/forward-zones/{name}")]
pub(crate) async fn update_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateForwardZoneRequest>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(path.into_inner())?;
    let request = payload.into_inner();
    let authz_requests = build_forward_zone_update_authz(&req, name.as_str(), &request);
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
    let refresh = request.refresh.map(SoaSeconds::new).transpose()?;
    let retry = request.retry.map(SoaSeconds::new).transpose()?;
    let expire = request.expire.map(SoaSeconds::new).transpose()?;
    let soa_ttl = request.soa_ttl.map(Ttl::new).transpose()?;
    let default_ttl = request.default_ttl.map(Ttl::new).transpose()?;
    let command = UpdateForwardZone {
        primary_ns,
        nameservers,
        email,
        refresh,
        retry,
        expire,
        soa_ttl,
        default_ttl,
    };
    let zone = state
        .services
        .zones()
        .update_forward(&name, command)
        .await?;
    Ok(HttpResponse::Ok().json(ForwardZoneResponse::from_domain(&zone)))
}

/// Delete a forward zone
#[utoipa::path(
    delete,
    path = "/api/v1/dns/forward-zones/{name}",
    params(("name" = String, Path, description = "Zone name")),
    responses(
        (status = 204, description = "Forward zone deleted"),
        (status = 404, description = "Forward zone not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/forward-zones/{name}")]
pub(crate) async fn delete_forward_zone(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = ZoneName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::zone::forward::DELETE,
            authz::actions::resource_kinds::FORWARD_ZONE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    state.services.zones().delete_forward(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_and_get_forward_zone() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        let create_ns = test::TestRequest::post()
            .uri("/dns/nameservers")
            .set_json(serde_json::json!({
                "name": "ns1.example.org",
                "ttl": 3600
            }))
            .to_request();
        let response = test::call_service(&app, create_ns).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let create_zone = test::TestRequest::post()
            .uri("/dns/forward-zones")
            .set_json(serde_json::json!({
                "name": "example.org",
                "primary_ns": "ns1.example.org",
                "nameservers": [],
                "email": "hostmaster@example.org"
            }))
            .to_request();
        let response = test::call_service(&app, create_zone).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["name"], "example.org");
        assert_eq!(body["primary_ns"], "ns1.example.org");
        assert_eq!(body["nameservers"], serde_json::json!(["ns1.example.org"]));

        let get_zone = test::TestRequest::get()
            .uri("/dns/forward-zones/example.org")
            .to_request();
        let response = test::call_service(&app, get_zone).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn zone_creation_auto_creates_ns_records() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        // Create nameserver and zone
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/nameservers")
                .set_json(serde_json::json!({"name": "ns1.nstest.org"}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/forward-zones")
                .set_json(serde_json::json!({
                    "name": "nstest.org",
                    "primary_ns": "ns1.nstest.org",
                    "nameservers": ["ns1.nstest.org"],
                    "email": "admin@nstest.org"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        // Verify NS record was auto-created
        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/dns/records").to_request(),
        )
        .await;
        let body: serde_json::Value = test::read_body_json(response).await;
        let records = body["items"].as_array().expect("records list");
        let ns_record = records
            .iter()
            .find(|r| r["type_name"] == "NS" && r["owner_name"] == "nstest.org");
        assert!(ns_record.is_some(), "NS record should be auto-created");
        assert_eq!(ns_record.unwrap()["data"]["nsdname"], "ns1.nstest.org");
    }
}
