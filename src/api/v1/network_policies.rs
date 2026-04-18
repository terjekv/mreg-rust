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
        filters::NetworkPolicyFilter,
        network_policy::NetworkPolicy,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::NetworkPolicyName,
    },
    errors::AppError,
};

use super::authz::{request as authz_request, require};

crate::page_response!(
    NetworkPolicyPageResponse,
    NetworkPolicyResponse,
    "Paginated list of network policies."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_network_policies)
        .service(create_network_policy)
        .service(get_network_policy)
        .service(delete_network_policy);
}

#[derive(Deserialize)]
pub struct PolicyQuery {
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    search: Option<String>,
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl PolicyQuery {
    fn into_parts(self) -> Result<(PageRequest, NetworkPolicyFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let mut filter = NetworkPolicyFilter::from_query_params(self.filters)?;
        filter.search = self.search;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateNetworkPolicyRequest {
    name: String,
    description: String,
    community_template_pattern: Option<String>,
}

impl CreateNetworkPolicyRequest {
    fn into_command(self) -> Result<crate::domain::network_policy::CreateNetworkPolicy, AppError> {
        crate::domain::network_policy::CreateNetworkPolicy::new(
            NetworkPolicyName::new(self.name)?,
            self.description,
            self.community_template_pattern,
        )
    }
}

#[derive(Serialize, ToSchema)]
pub struct NetworkPolicyResponse {
    id: Uuid,
    name: String,
    description: String,
    community_template_pattern: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NetworkPolicyResponse {
    fn from_domain(value: &NetworkPolicy) -> Self {
        Self {
            id: value.id(),
            name: value.name().as_str().to_string(),
            description: value.description().to_string(),
            community_template_pattern: value.community_template_pattern().map(str::to_string),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

/// List network policies
#[utoipa::path(
    get,
    path = "/api/v1/policy/network/policies",
    responses(
        (status = 200, description = "Paginated list of network policies", body = NetworkPolicyPageResponse)
    ),
    tag = "Policy"
)]
#[get("/policy/network/policies")]
pub(crate) async fn list_network_policies(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PolicyQuery>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            authz::actions::network_policy::LIST,
            authz::actions::resource_kinds::NETWORK_POLICY,
            "*",
        ),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state
        .services
        .network_policies()
        .list(&page, &filter)
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        NetworkPolicyResponse::from_domain,
    )))
}

/// Create a network policy
#[utoipa::path(
    post,
    path = "/api/v1/policy/network/policies",
    request_body = CreateNetworkPolicyRequest,
    responses(
        (status = 201, description = "Network policy created", body = NetworkPolicyResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Policy already exists")
    ),
    tag = "Policy"
)]
#[post("/policy/network/policies")]
pub(crate) async fn create_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateNetworkPolicyRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::network_policy::CREATE,
        authz::actions::resource_kinds::NETWORK_POLICY,
        request.name.clone(),
    )
    .attr(
        "description",
        AttrValue::String(request.description.clone()),
    );
    if let Some(pattern) = &request.community_template_pattern {
        authz = authz.attr(
            "community_template_pattern",
            AttrValue::String(pattern.clone()),
        );
    }
    require(&state, authz).await?;
    let item = state
        .services
        .network_policies()
        .create(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(NetworkPolicyResponse::from_domain(&item)))
}

/// Get a network policy by name
#[utoipa::path(
    get,
    path = "/api/v1/policy/network/policies/{name}",
    params(("name" = String, Path, description = "Policy name")),
    responses(
        (status = 200, description = "Network policy found", body = NetworkPolicyResponse),
        (status = 404, description = "Network policy not found")
    ),
    tag = "Policy"
)]
#[get("/policy/network/policies/{name}")]
pub(crate) async fn get_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = NetworkPolicyName::new(path.into_inner())?;
    require(
        &state,
        authz_request(
            &req,
            authz::actions::network_policy::GET,
            authz::actions::resource_kinds::NETWORK_POLICY,
            name.as_str(),
        ),
    )
    .await?;
    let item = state.services.network_policies().get(&name).await?;
    Ok(HttpResponse::Ok().json(NetworkPolicyResponse::from_domain(&item)))
}

/// Delete a network policy
#[utoipa::path(
    delete,
    path = "/api/v1/policy/network/policies/{name}",
    params(("name" = String, Path, description = "Policy name")),
    responses(
        (status = 204, description = "Network policy deleted"),
        (status = 404, description = "Network policy not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/network/policies/{name}")]
pub(crate) async fn delete_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = NetworkPolicyName::new(path.into_inner())?;
    require(
        &state,
        authz_request(
            &req,
            authz::actions::network_policy::DELETE,
            authz::actions::resource_kinds::NETWORK_POLICY,
            name.as_str(),
        ),
    )
    .await?;
    state.services.network_policies().delete(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}
