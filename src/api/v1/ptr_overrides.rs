use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, delete, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        filters::PtrOverrideFilter,
        pagination::{PageRequest, PageResponse, SortDirection},
        ptr_override::PtrOverride,
        types::{DnsName, Hostname, IpAddressValue},
    },
    errors::AppError,
    services::ptr_overrides as ptr_override_service,
};

use super::authz::request as authz_request;

crate::page_response!(
    PtrOverridePageResponse,
    PtrOverrideResponse,
    "Paginated list of PTR overrides."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_ptr_overrides)
        .service(create_ptr_override)
        .service(get_ptr_override)
        .service(delete_ptr_override);
}

#[derive(Deserialize)]
pub struct PtrQuery {
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl PtrQuery {
    fn into_parts(self) -> Result<(PageRequest, PtrOverrideFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let filter = PtrOverrideFilter::from_query_params(self.filters)?;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreatePtrOverrideRequest {
    host_name: String,
    address: String,
    target_name: Option<String>,
}

impl CreatePtrOverrideRequest {
    fn into_command(self) -> Result<crate::domain::ptr_override::CreatePtrOverride, AppError> {
        Ok(crate::domain::ptr_override::CreatePtrOverride::new(
            Hostname::new(self.host_name)?,
            IpAddressValue::new(self.address)?,
            self.target_name.map(DnsName::new).transpose()?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct PtrOverrideResponse {
    id: Uuid,
    host_name: String,
    address: String,
    target_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PtrOverrideResponse {
    fn from_domain(value: &PtrOverride) -> Self {
        Self {
            id: value.id(),
            host_name: value.host_name().as_str().to_string(),
            address: value.address().as_str(),
            target_name: value.target_name().map(|name| name.as_str().to_string()),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

/// List PTR overrides
#[utoipa::path(
    get,
    path = "/api/v1/dns/ptr-overrides",
    responses(
        (status = 200, description = "Paginated list of PTR overrides", body = PtrOverridePageResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/ptr-overrides")]
pub(crate) async fn list_ptr_overrides(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PtrQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::ptr_override::LIST,
            authz::actions::resource_kinds::PTR_OVERRIDE,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result =
        ptr_override_service::list_ptr_overrides(state.storage.ptr_overrides(), &page, &filter)
            .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        PtrOverrideResponse::from_domain,
    )))
}

/// Create a PTR override
#[utoipa::path(
    post,
    path = "/api/v1/dns/ptr-overrides",
    request_body = CreatePtrOverrideRequest,
    responses(
        (status = 201, description = "PTR override created", body = PtrOverrideResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "PTR override already exists")
    ),
    tag = "DNS"
)]
#[post("/dns/ptr-overrides")]
pub(crate) async fn create_ptr_override(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreatePtrOverrideRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::ptr_override::CREATE,
        authz::actions::resource_kinds::PTR_OVERRIDE,
        request.address.clone(),
    )
    .attr("host_name", AttrValue::String(request.host_name.clone()))
    .attr("address", AttrValue::Ip(request.address.clone()));
    if let Some(target_name) = &request.target_name {
        authz = authz.attr("target_name", AttrValue::String(target_name.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let item = ptr_override_service::create_ptr_override(
        state.storage.ptr_overrides(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created().json(PtrOverrideResponse::from_domain(&item)))
}

/// Get a PTR override by address
#[utoipa::path(
    get,
    path = "/api/v1/dns/ptr-overrides/{address}",
    params(("address" = String, Path, description = "IP address")),
    responses(
        (status = 200, description = "PTR override found", body = PtrOverrideResponse),
        (status = 404, description = "PTR override not found")
    ),
    tag = "DNS"
)]
#[get("/dns/ptr-overrides/{address:.*}")]
pub(crate) async fn get_ptr_override(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let address = IpAddressValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::ptr_override::GET,
            authz::actions::resource_kinds::PTR_OVERRIDE,
            address.as_str(),
        )
        .build(),
    )
    .await?;
    let item =
        ptr_override_service::get_ptr_override(state.storage.ptr_overrides(), &address).await?;
    Ok(HttpResponse::Ok().json(PtrOverrideResponse::from_domain(&item)))
}

/// Delete a PTR override
#[utoipa::path(
    delete,
    path = "/api/v1/dns/ptr-overrides/{address}",
    params(("address" = String, Path, description = "IP address")),
    responses(
        (status = 204, description = "PTR override deleted"),
        (status = 404, description = "PTR override not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/ptr-overrides/{address:.*}")]
pub(crate) async fn delete_ptr_override(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let address = IpAddressValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::ptr_override::DELETE,
            authz::actions::resource_kinds::PTR_OVERRIDE,
            address.as_str(),
        )
        .build(),
    )
    .await?;
    ptr_override_service::delete_ptr_override(
        state.storage.ptr_overrides(),
        state.storage.audit(),
        &state.events,
        &address,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
