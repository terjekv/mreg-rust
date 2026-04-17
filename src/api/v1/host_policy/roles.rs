use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        host_policy::{CreateHostPolicyRole, HostPolicyRole, UpdateHostPolicyRole},
        pagination::{PageRequest, PageResponse},
        types::HostPolicyName,
    },
    errors::AppError,
    services::host_policy as hp_service,
};

use crate::api::v1::authz::request as authz_request;

crate::page_response!(
    RolePageResponse,
    RoleResponse,
    "Paginated list of host-policy roles."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_roles)
        .service(create_role)
        .service(get_role)
        .service(update_role)
        .service(delete_role);
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for creating a host-policy role.
#[derive(Deserialize, ToSchema)]
pub struct CreateRoleRequest {
    name: String,
    description: String,
}

impl CreateRoleRequest {
    fn into_command(self) -> Result<CreateHostPolicyRole, AppError> {
        Ok(CreateHostPolicyRole::new(
            HostPolicyName::new(self.name)?,
            self.description,
        ))
    }
}

/// Request body for updating a host-policy role.
#[derive(Deserialize, ToSchema)]
pub struct UpdateRoleRequest {
    description: Option<String>,
}

/// Response body for a host-policy role.
#[derive(Serialize, ToSchema)]
pub struct RoleResponse {
    id: Uuid,
    name: String,
    description: String,
    atoms: Vec<String>,
    hosts: Vec<String>,
    labels: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RoleResponse {
    pub(crate) fn from_domain(role: &HostPolicyRole) -> Self {
        Self {
            id: role.id(),
            name: role.name().as_str().to_string(),
            description: role.description().to_string(),
            atoms: role.atoms().to_vec(),
            hosts: role.hosts().to_vec(),
            labels: role.labels().to_vec(),
            created_at: role.created_at(),
            updated_at: role.updated_at(),
        }
    }
}

// ---------------------------------------------------------------------------
// Role endpoints
// ---------------------------------------------------------------------------

/// List all host-policy roles
#[utoipa::path(
    get,
    path = "/api/v1/policy/host/roles",
    params(PageRequest),
    responses(
        (status = 200, description = "Paginated list of roles", body = RolePageResponse)
    ),
    tag = "Policy"
)]
#[get("/policy/host/roles")]
pub(crate) async fn list_roles(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::LIST,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            "*",
        )
        .build(),
    )
    .await?;
    let page = hp_service::list_roles(state.storage.host_policy(), &query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(page, RoleResponse::from_domain)))
}

/// Create a new host-policy role
#[utoipa::path(
    post,
    path = "/api/v1/policy/host/roles",
    request_body = CreateRoleRequest,
    responses(
        (status = 201, description = "Role created", body = RoleResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Role already exists")
    ),
    tag = "Policy"
)]
#[post("/policy/host/roles")]
pub(crate) async fn create_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateRoleRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::CREATE,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            request.name.clone(),
        )
        .attr(
            "description",
            AttrValue::String(request.description.clone()),
        )
        .build(),
    )
    .await?;
    let role = hp_service::create_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created().json(RoleResponse::from_domain(&role)))
}

/// Get a host-policy role by name (includes atoms, hosts, labels)
#[utoipa::path(
    get,
    path = "/api/v1/policy/host/roles/{name}",
    params(("name" = String, Path, description = "Role name")),
    responses(
        (status = 200, description = "Role found", body = RoleResponse),
        (status = 404, description = "Role not found")
    ),
    tag = "Policy"
)]
#[get("/policy/host/roles/{name}")]
pub(crate) async fn get_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::GET,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let role = hp_service::get_role(state.storage.host_policy(), &name).await?;
    Ok(HttpResponse::Ok().json(RoleResponse::from_domain(&role)))
}

/// Update a host-policy role's description
#[utoipa::path(
    patch,
    path = "/api/v1/policy/host/roles/{name}",
    params(("name" = String, Path, description = "Role name")),
    request_body = UpdateRoleRequest,
    responses(
        (status = 200, description = "Role updated", body = RoleResponse),
        (status = 404, description = "Role not found")
    ),
    tag = "Policy"
)]
#[patch("/policy/host/roles/{name}")]
pub(crate) async fn update_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateRoleRequest>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(path.into_inner())?;
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::host_policy::role::UPDATE_DESCRIPTION,
        authz::actions::resource_kinds::HOST_POLICY_ROLE,
        name.as_str(),
    );
    if let Some(description) = &request.description {
        authz = authz.attr("new_description", AttrValue::String(description.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let command = UpdateHostPolicyRole {
        description: request.description,
    };
    let role = hp_service::update_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &name,
        command,
    )
    .await?;
    Ok(HttpResponse::Ok().json(RoleResponse::from_domain(&role)))
}

/// Delete a host-policy role
#[utoipa::path(
    delete,
    path = "/api/v1/policy/host/roles/{name}",
    params(("name" = String, Path, description = "Role name")),
    responses(
        (status = 204, description = "Role deleted"),
        (status = 404, description = "Role not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/host/roles/{name}")]
pub(crate) async fn delete_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::DELETE,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    hp_service::delete_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
