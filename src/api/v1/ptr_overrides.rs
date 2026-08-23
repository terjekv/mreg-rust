use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, delete, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue},
    domain::{
        filters::PtrOverrideFilter,
        pagination::{PageRequest, PageResponse, SortDirection},
        ptr_override::PtrOverride,
        types::{DnsName, Hostname, IpAddressValue},
    },
    errors::AppError,
};

use super::authz::{request as authz_request, require};

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
    after: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::domain::pagination::deserialize_page_limit"
    )]
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

async fn require_ptr_override_permission(
    state: &AppState,
    req: &HttpRequest,
    action: &str,
    host_name: &Hostname,
    address: &IpAddressValue,
    target_name: Option<&DnsName>,
) -> Result<(), AppError> {
    let assignment = state.services.hosts().get_ip_address(address).await?;
    let attachment = state
        .services
        .attachments()
        .get_attachment(assignment.attachment_id())
        .await?;
    if attachment.host_name() != host_name {
        return Err(AppError::validation(
            "PTR override address must be assigned to the supplied host",
        ));
    }
    let mut authorization = authz_request(
        req,
        action,
        authz::actions::resource_kinds::PTR_OVERRIDE,
        address.as_str(),
    )
    .attr(
        "host_name",
        AttrValue::String(attachment.host_name().as_str().to_string()),
    )
    .attr("address", AttrValue::Ip(address.as_str()))
    .attr("network", AttrValue::Ip(attachment.network_cidr().as_str()))
    .attr(
        "attachment_id",
        AttrValue::String(attachment.id().to_string()),
    );
    if let Some(target_name) = target_name {
        authorization = authorization.attr(
            "target_name",
            AttrValue::String(target_name.as_str().to_string()),
        );
    }
    require(state, authorization).await
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
    require(
        &state,
        authz_request(
            &req,
            authz::actions::ptr_override::LIST,
            authz::actions::resource_kinds::PTR_OVERRIDE,
            "*",
        ),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state.services.ptr_overrides().list(&page, &filter).await?;
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
    let command = payload.into_inner().into_command()?;
    require_ptr_override_permission(
        state.get_ref(),
        &req,
        authz::actions::ptr_override::CREATE,
        command.host_name(),
        command.address(),
        command.target_name(),
    )
    .await?;
    let item = state.services.ptr_overrides().create(command).await?;
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
    let item = state.services.ptr_overrides().get(&address).await?;
    require_ptr_override_permission(
        state.get_ref(),
        &req,
        authz::actions::ptr_override::GET,
        item.host_name(),
        item.address(),
        item.target_name(),
    )
    .await?;
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
    let item = state.services.ptr_overrides().get(&address).await?;
    require_ptr_override_permission(
        state.get_ref(),
        &req,
        authz::actions::ptr_override::DELETE,
        item.host_name(),
        item.address(),
        item.target_name(),
    )
    .await?;
    state.services.ptr_overrides().delete(&address).await?;
    Ok(HttpResponse::NoContent().finish())
}
