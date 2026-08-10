use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue},
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{
            CreateNetworkPolicyAttribute, NetworkPolicyAttribute, NetworkPolicyDetails,
            SetNetworkPolicyAttributeValue, UpdateNetworkPolicy, UpdateNetworkPolicyAttribute,
        },
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{NetworkPolicyAttributeName, NetworkPolicyName, UpdateField},
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
        .service(update_network_policy)
        .service(delete_network_policy)
        .service(list_network_policy_attributes)
        .service(create_network_policy_attribute)
        .service(get_network_policy_attribute)
        .service(update_network_policy_attribute)
        .service(delete_network_policy_attribute);
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
    #[serde(default)]
    description: String,
    community_template_pattern: Option<String>,
    #[serde(default)]
    attributes: Vec<NetworkPolicyAttributeValueRequest>,
}

impl CreateNetworkPolicyRequest {
    fn into_command(self) -> Result<crate::domain::network_policy::CreateNetworkPolicy, AppError> {
        let description = required_description(self.description, "network policy description")?;
        Ok(crate::domain::network_policy::CreateNetworkPolicy::new(
            NetworkPolicyName::new(self.name)?,
            description,
            self.community_template_pattern,
        )?
        .with_attributes(
            self.attributes
                .into_iter()
                .map(NetworkPolicyAttributeValueRequest::into_domain)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
pub struct NetworkPolicyAttributeValueRequest {
    name: String,
    value: bool,
}

impl NetworkPolicyAttributeValueRequest {
    fn into_domain(self) -> Result<SetNetworkPolicyAttributeValue, AppError> {
        Ok(SetNetworkPolicyAttributeValue::new(
            NetworkPolicyAttributeName::new(self.name)?,
            self.value,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct NetworkPolicyAttributeValueResponse {
    name: String,
    value: bool,
}

#[derive(Serialize, ToSchema)]
pub struct NetworkPolicyResponse {
    id: Uuid,
    name: String,
    description: String,
    community_template_pattern: Option<String>,
    attributes: Vec<NetworkPolicyAttributeValueResponse>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NetworkPolicyResponse {
    fn from_domain(value: &NetworkPolicyDetails) -> Self {
        let policy = value.policy();
        Self {
            id: policy.id(),
            name: policy.name().as_str().to_string(),
            description: policy.description().to_string(),
            community_template_pattern: policy.community_template_pattern().map(str::to_string),
            attributes: value
                .attributes()
                .iter()
                .map(|value| NetworkPolicyAttributeValueResponse {
                    name: value.name().as_str().to_string(),
                    value: value.value(),
                })
                .collect(),
            created_at: policy.created_at(),
            updated_at: policy.updated_at(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateNetworkPolicyRequest {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    community_template_pattern: UpdateField<String>,
    attributes: Option<Vec<NetworkPolicyAttributeValueRequest>>,
}

impl UpdateNetworkPolicyRequest {
    fn into_domain(self) -> Result<UpdateNetworkPolicy, AppError> {
        Ok(UpdateNetworkPolicy {
            name: self.name.map(NetworkPolicyName::new).transpose()?,
            description: self
                .description
                .map(|value| required_description(value, "network policy description"))
                .transpose()?,
            community_template_pattern: self.community_template_pattern,
            attributes: self
                .attributes
                .map(|values| {
                    values
                        .into_iter()
                        .map(NetworkPolicyAttributeValueRequest::into_domain)
                        .collect()
                })
                .transpose()?,
        })
    }
}

fn required_description(value: String, label: &str) -> Result<String, AppError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::validation(format!("{label} cannot be empty")));
    }
    Ok(value)
}

#[derive(Deserialize, ToSchema)]
pub struct CreateNetworkPolicyAttributeRequest {
    name: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateNetworkPolicyAttributeRequest {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct NetworkPolicyAttributeResponse {
    id: Uuid,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NetworkPolicyAttributeResponse {
    fn from_domain(value: &NetworkPolicyAttribute) -> Self {
        Self {
            id: value.id(),
            name: value.name().as_str().to_string(),
            description: value.description().to_string(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

crate::page_response!(
    NetworkPolicyAttributePageResponse,
    NetworkPolicyAttributeResponse,
    "Paginated list of network policy attributes."
);

/// List network policies
#[utoipa::path(
    get,
    path = "/api/v2/policy/network/policies",
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
    let mut responses = Vec::with_capacity(result.items.len());
    for policy in result.items {
        let details = state
            .services
            .network_policies()
            .get_details(policy.name())
            .await?;
        responses.push(NetworkPolicyResponse::from_domain(&details));
    }
    Ok(HttpResponse::Ok().json(PageResponse {
        items: responses,
        total: result.total,
        next_cursor: result.next_cursor,
    }))
}

/// Create a network policy
#[utoipa::path(
    post,
    path = "/api/v2/policy/network/policies",
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
    let details = state
        .services
        .network_policies()
        .get_details(item.name())
        .await?;
    Ok(HttpResponse::Created().json(NetworkPolicyResponse::from_domain(&details)))
}

/// Get a network policy by name
#[utoipa::path(
    get,
    path = "/api/v2/policy/network/policies/{name}",
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
    let item = state.services.network_policies().get_details(&name).await?;
    Ok(HttpResponse::Ok().json(NetworkPolicyResponse::from_domain(&item)))
}

/// Update a network policy and optionally replace all attribute values.
#[utoipa::path(
    patch,
    path = "/api/v2/policy/network/policies/{name}",
    params(("name" = String, Path, description = "Policy name")),
    responses((status = 200, body = NetworkPolicyResponse)),
    tag = "Policy"
)]
#[patch("/policy/network/policies/{name}")]
pub(crate) async fn update_network_policy(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateNetworkPolicyRequest>,
) -> Result<HttpResponse, AppError> {
    let name = NetworkPolicyName::new(path.into_inner())?;
    require(
        &state,
        authz_request(
            &req,
            authz::actions::network_policy::UPDATE,
            authz::actions::resource_kinds::NETWORK_POLICY,
            name.as_str(),
        ),
    )
    .await?;
    let item = state
        .services
        .network_policies()
        .update(&name, payload.into_inner().into_domain()?)
        .await?;
    let details = state
        .services
        .network_policies()
        .get_details(item.name())
        .await?;
    Ok(HttpResponse::Ok().json(NetworkPolicyResponse::from_domain(&details)))
}

/// Delete a network policy
#[utoipa::path(
    delete,
    path = "/api/v2/policy/network/policies/{name}",
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

/// List network-policy attribute definitions.
#[utoipa::path(
    get,
    path = "/api/v2/policy/network/attributes",
    responses((status = 200, body = NetworkPolicyAttributePageResponse)),
    tag = "Policy"
)]
#[get("/policy/network/attributes")]
pub(crate) async fn list_network_policy_attributes(
    req: HttpRequest,
    state: web::Data<AppState>,
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
    let page = state
        .services
        .network_policies()
        .list_attributes(&PageRequest::all())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        NetworkPolicyAttributeResponse::from_domain,
    )))
}

/// Create a network-policy attribute definition.
#[utoipa::path(
    post,
    path = "/api/v2/policy/network/attributes",
    request_body = CreateNetworkPolicyAttributeRequest,
    responses((status = 201, body = NetworkPolicyAttributeResponse)),
    tag = "Policy"
)]
#[post("/policy/network/attributes")]
pub(crate) async fn create_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateNetworkPolicyAttributeRequest>,
) -> Result<HttpResponse, AppError> {
    let payload = payload.into_inner();
    let name = NetworkPolicyAttributeName::new(payload.name)?;
    require(
        &state,
        authz_request(
            &req,
            authz::actions::network_policy::CREATE,
            authz::actions::resource_kinds::NETWORK_POLICY,
            name.as_str(),
        ),
    )
    .await?;
    let item = state
        .services
        .network_policies()
        .create_attribute(CreateNetworkPolicyAttribute::new(name, payload.description))
        .await?;
    Ok(HttpResponse::Created().json(NetworkPolicyAttributeResponse::from_domain(&item)))
}

/// Get a network-policy attribute definition by name.
#[utoipa::path(
    get,
    path = "/api/v2/policy/network/attributes/{name}",
    params(("name" = String, Path)),
    responses((status = 200, body = NetworkPolicyAttributeResponse)),
    tag = "Policy"
)]
#[get("/policy/network/attributes/{name}")]
pub(crate) async fn get_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = NetworkPolicyAttributeName::new(path.into_inner())?;
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
    let item = state
        .services
        .network_policies()
        .get_attribute(&name)
        .await?;
    Ok(HttpResponse::Ok().json(NetworkPolicyAttributeResponse::from_domain(&item)))
}

/// Update a network-policy attribute definition.
#[utoipa::path(
    patch,
    path = "/api/v2/policy/network/attributes/{name}",
    params(("name" = String, Path)),
    request_body = UpdateNetworkPolicyAttributeRequest,
    responses((status = 200, body = NetworkPolicyAttributeResponse)),
    tag = "Policy"
)]
#[patch("/policy/network/attributes/{name}")]
pub(crate) async fn update_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateNetworkPolicyAttributeRequest>,
) -> Result<HttpResponse, AppError> {
    let name = NetworkPolicyAttributeName::new(path.into_inner())?;
    require(
        &state,
        authz_request(
            &req,
            authz::actions::network_policy::UPDATE,
            authz::actions::resource_kinds::NETWORK_POLICY,
            name.as_str(),
        ),
    )
    .await?;
    let payload = payload.into_inner();
    let item = state
        .services
        .network_policies()
        .update_attribute(
            &name,
            UpdateNetworkPolicyAttribute {
                name: payload
                    .name
                    .map(NetworkPolicyAttributeName::new)
                    .transpose()?,
                description: payload.description,
            },
        )
        .await?;
    Ok(HttpResponse::Ok().json(NetworkPolicyAttributeResponse::from_domain(&item)))
}

/// Delete a network-policy attribute definition and its policy memberships.
#[utoipa::path(
    delete,
    path = "/api/v2/policy/network/attributes/{name}",
    params(("name" = String, Path)),
    responses((status = 204)),
    tag = "Policy"
)]
#[delete("/policy/network/attributes/{name}")]
pub(crate) async fn delete_network_policy_attribute(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = NetworkPolicyAttributeName::new(path.into_inner())?;
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
    state
        .services
        .network_policies()
        .delete_attribute(&name)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}
