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
        bacnet::BacnetIdAssignment,
        filters::BacnetIdFilter,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{BacnetIdentifier, Hostname},
    },
    errors::AppError,
};

use super::authz::request as authz_request;

crate::page_response!(
    BacnetPageResponse,
    BacnetResponse,
    "Paginated list of BACnet ID assignments."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_bacnet_ids)
        .service(create_bacnet_id)
        .service(get_bacnet_id)
        .service(delete_bacnet_id);
}

#[derive(Deserialize)]
pub struct BacnetQuery {
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl BacnetQuery {
    fn into_parts(self) -> Result<(PageRequest, BacnetIdFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let filter = BacnetIdFilter::from_query_params(self.filters)?;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateBacnetRequest {
    bacnet_id: u32,
    host_name: String,
}

impl CreateBacnetRequest {
    fn into_command(self) -> Result<crate::domain::bacnet::CreateBacnetIdAssignment, AppError> {
        Ok(crate::domain::bacnet::CreateBacnetIdAssignment::new(
            BacnetIdentifier::new(self.bacnet_id)?,
            Hostname::new(self.host_name)?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct BacnetResponse {
    bacnet_id: u32,
    host_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl BacnetResponse {
    fn from_domain(value: &BacnetIdAssignment) -> Self {
        Self {
            bacnet_id: value.bacnet_id().as_u32(),
            host_name: value.host_name().as_str().to_string(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

/// List BACnet ID assignments
#[utoipa::path(
    get,
    path = "/api/v1/inventory/bacnet-ids",
    responses(
        (status = 200, description = "Paginated list of BACnet ID assignments", body = BacnetPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/bacnet-ids")]
pub(crate) async fn list_bacnet_ids(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<BacnetQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::bacnet_id::LIST,
            authz::actions::resource_kinds::BACNET_ID,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state.services.bacnet().list(&page, &filter).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(result, BacnetResponse::from_domain)))
}

/// Create a BACnet ID assignment
#[utoipa::path(
    post,
    path = "/api/v1/inventory/bacnet-ids",
    request_body = CreateBacnetRequest,
    responses(
        (status = 201, description = "BACnet ID assigned", body = BacnetResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "BACnet ID already assigned")
    ),
    tag = "Inventory"
)]
#[post("/inventory/bacnet-ids")]
pub(crate) async fn create_bacnet_id(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateBacnetRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::bacnet_id::CREATE,
            authz::actions::resource_kinds::BACNET_ID,
            request.bacnet_id.to_string(),
        )
        .attr("bacnet_id", AttrValue::Long(i64::from(request.bacnet_id)))
        .attr("host_name", AttrValue::String(request.host_name.clone()))
        .build(),
    )
    .await?;
    let item = state
        .services
        .bacnet()
        .create(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(BacnetResponse::from_domain(&item)))
}

/// Get a BACnet ID assignment
#[utoipa::path(
    get,
    path = "/api/v1/inventory/bacnet-ids/{bacnet_id}",
    params(("bacnet_id" = u32, Path, description = "BACnet ID")),
    responses(
        (status = 200, description = "BACnet ID found", body = BacnetResponse),
        (status = 404, description = "BACnet ID not found")
    ),
    tag = "Inventory"
)]
#[get("/inventory/bacnet-ids/{bacnet_id}")]
pub(crate) async fn get_bacnet_id(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let bacnet_id = BacnetIdentifier::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::bacnet_id::GET,
            authz::actions::resource_kinds::BACNET_ID,
            bacnet_id.as_u32().to_string(),
        )
        .build(),
    )
    .await?;
    let item = state.services.bacnet().get(bacnet_id).await?;
    Ok(HttpResponse::Ok().json(BacnetResponse::from_domain(&item)))
}

/// Delete a BACnet ID assignment
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/bacnet-ids/{bacnet_id}",
    params(("bacnet_id" = u32, Path, description = "BACnet ID")),
    responses(
        (status = 204, description = "BACnet ID deleted"),
        (status = 404, description = "BACnet ID not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/bacnet-ids/{bacnet_id}")]
pub(crate) async fn delete_bacnet_id(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<u32>,
) -> Result<HttpResponse, AppError> {
    let bacnet_id = BacnetIdentifier::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::bacnet_id::DELETE,
            authz::actions::resource_kinds::BACNET_ID,
            bacnet_id.as_u32().to_string(),
        )
        .build(),
    )
    .await?;
    state.services.bacnet().delete(bacnet_id).await?;
    Ok(HttpResponse::NoContent().finish())
}
