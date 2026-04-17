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
        filters::HostGroupFilter,
        host_group::HostGroup,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{HostGroupName, Hostname, OwnerGroupName},
    },
    errors::AppError,
    services::host_groups as host_group_service,
};

use super::authz::{request as authz_request, string_set};

crate::page_response!(
    HostGroupPageResponse,
    HostGroupResponse,
    "Paginated list of host groups."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_host_groups)
        .service(create_host_group)
        .service(get_host_group)
        .service(delete_host_group);
}

#[derive(Deserialize)]
pub struct HostGroupQuery {
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    search: Option<String>,
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl HostGroupQuery {
    fn into_parts(self) -> Result<(PageRequest, HostGroupFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let mut filter = HostGroupFilter::from_query_params(self.filters)?;
        filter.search = self.search;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateHostGroupRequest {
    name: String,
    description: String,
    #[serde(default)]
    hosts: Vec<String>,
    #[serde(default)]
    parent_groups: Vec<String>,
    #[serde(default)]
    owner_groups: Vec<String>,
}

impl CreateHostGroupRequest {
    fn into_command(self) -> Result<crate::domain::host_group::CreateHostGroup, AppError> {
        crate::domain::host_group::CreateHostGroup::new(
            HostGroupName::new(self.name)?,
            self.description,
            self.hosts
                .into_iter()
                .map(Hostname::new)
                .collect::<Result<Vec<_>, _>>()?,
            self.parent_groups
                .into_iter()
                .map(HostGroupName::new)
                .collect::<Result<Vec<_>, _>>()?,
            self.owner_groups
                .into_iter()
                .map(OwnerGroupName::new)
                .collect::<Result<Vec<_>, _>>()?,
        )
    }
}

#[derive(Serialize, ToSchema)]
pub struct HostGroupResponse {
    id: Uuid,
    name: String,
    description: String,
    hosts: Vec<String>,
    parent_groups: Vec<String>,
    owner_groups: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostGroupResponse {
    fn from_domain(group: &HostGroup) -> Self {
        Self {
            id: group.id(),
            name: group.name().as_str().to_string(),
            description: group.description().to_string(),
            hosts: group
                .hosts()
                .iter()
                .map(|host| host.as_str().to_string())
                .collect(),
            parent_groups: group
                .parent_groups()
                .iter()
                .map(|group| group.as_str().to_string())
                .collect(),
            owner_groups: group
                .owner_groups()
                .iter()
                .map(|group| group.as_str().to_string())
                .collect(),
            created_at: group.created_at(),
            updated_at: group.updated_at(),
        }
    }
}

/// List host groups
#[utoipa::path(
    get,
    path = "/api/v1/inventory/host-groups",
    responses(
        (status = 200, description = "Paginated list of host groups", body = HostGroupPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/host-groups")]
pub(crate) async fn list_host_groups(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<HostGroupQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_group::LIST,
            authz::actions::resource_kinds::HOST_GROUP,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result =
        host_group_service::list_host_groups(state.storage.host_groups(), &page, &filter).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        HostGroupResponse::from_domain,
    )))
}

/// Create a host group
#[utoipa::path(
    post,
    path = "/api/v1/inventory/host-groups",
    request_body = CreateHostGroupRequest,
    responses(
        (status = 201, description = "Host group created", body = HostGroupResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Host group already exists")
    ),
    tag = "Inventory"
)]
#[post("/inventory/host-groups")]
pub(crate) async fn create_host_group(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateHostGroupRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_group::CREATE,
            authz::actions::resource_kinds::HOST_GROUP,
            request.name.clone(),
        )
        .attr("hosts", string_set(request.hosts.clone()))
        .attr("parent_groups", string_set(request.parent_groups.clone()))
        .attr("owner_groups", string_set(request.owner_groups.clone()))
        .attr(
            "description",
            AttrValue::String(request.description.clone()),
        )
        .build(),
    )
    .await?;
    let group = host_group_service::create_host_group(
        state.storage.host_groups(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created().json(HostGroupResponse::from_domain(&group)))
}

/// Get a host group by name
#[utoipa::path(
    get,
    path = "/api/v1/inventory/host-groups/{name}",
    params(("name" = String, Path, description = "Host group name")),
    responses(
        (status = 200, description = "Host group found", body = HostGroupResponse),
        (status = 404, description = "Host group not found")
    ),
    tag = "Inventory"
)]
#[get("/inventory/host-groups/{name}")]
pub(crate) async fn get_host_group(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostGroupName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_group::GET,
            authz::actions::resource_kinds::HOST_GROUP,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let group = host_group_service::get_host_group(state.storage.host_groups(), &name).await?;
    Ok(HttpResponse::Ok().json(HostGroupResponse::from_domain(&group)))
}

/// Delete a host group
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/host-groups/{name}",
    params(("name" = String, Path, description = "Host group name")),
    responses(
        (status = 204, description = "Host group deleted"),
        (status = 404, description = "Host group not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/host-groups/{name}")]
pub(crate) async fn delete_host_group(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostGroupName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_group::DELETE,
            authz::actions::resource_kinds::HOST_GROUP,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    host_group_service::delete_host_group(
        state.storage.host_groups(),
        state.storage.audit(),
        &state.events,
        &name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
