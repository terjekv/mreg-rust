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
        community::Community,
        filters::CommunityFilter,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
    services::communities as community_service,
};

use super::authz::request as authz_request;

crate::page_response!(
    CommunityPageResponse,
    CommunityResponse,
    "Paginated list of communities."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_communities)
        .service(create_community)
        .service(get_community)
        .service(delete_community);
}

#[derive(Deserialize)]
pub struct CommunityQuery {
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    search: Option<String>,
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl CommunityQuery {
    fn into_parts(self) -> Result<(PageRequest, CommunityFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let mut filter = CommunityFilter::from_query_params(self.filters)?;
        filter.search = self.search;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateCommunityRequest {
    policy_name: String,
    network: String,
    name: String,
    description: String,
}

impl CreateCommunityRequest {
    fn into_command(self) -> Result<crate::domain::community::CreateCommunity, AppError> {
        crate::domain::community::CreateCommunity::new(
            NetworkPolicyName::new(self.policy_name)?,
            crate::domain::types::CidrValue::new(self.network)?,
            CommunityName::new(self.name)?,
            self.description,
        )
    }
}

#[derive(Serialize, ToSchema)]
pub struct CommunityResponse {
    id: Uuid,
    policy_id: Uuid,
    policy_name: String,
    network: String,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl CommunityResponse {
    fn from_domain(value: &Community) -> Self {
        Self {
            id: value.id(),
            policy_id: value.policy_id(),
            policy_name: value.policy_name().as_str().to_string(),
            network: value.network_cidr().as_str(),
            name: value.name().as_str().to_string(),
            description: value.description().to_string(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

/// List communities
#[utoipa::path(
    get,
    path = "/api/v1/policy/network/communities",
    responses(
        (status = 200, description = "Paginated list of communities", body = CommunityPageResponse)
    ),
    tag = "Policy"
)]
#[get("/policy/network/communities")]
pub(crate) async fn list_communities(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<CommunityQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::community::LIST,
            authz::actions::resource_kinds::COMMUNITY,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result =
        community_service::list_communities(state.storage.communities(), &page, &filter).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        CommunityResponse::from_domain,
    )))
}

/// Create a community
#[utoipa::path(
    post,
    path = "/api/v1/policy/network/communities",
    request_body = CreateCommunityRequest,
    responses(
        (status = 201, description = "Community created", body = CommunityResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Community already exists")
    ),
    tag = "Policy"
)]
#[post("/policy/network/communities")]
pub(crate) async fn create_community(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateCommunityRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::community::CREATE,
            authz::actions::resource_kinds::COMMUNITY,
            request.name.clone(),
        )
        .attr(
            "policy_name",
            AttrValue::String(request.policy_name.clone()),
        )
        .attr("network", AttrValue::Ip(request.network.clone()))
        .attr(
            "description",
            AttrValue::String(request.description.clone()),
        )
        .build(),
    )
    .await?;
    let item = community_service::create_community(
        state.storage.communities(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created().json(CommunityResponse::from_domain(&item)))
}

/// Get a community by ID
#[utoipa::path(
    get,
    path = "/api/v1/policy/network/communities/{community_id}",
    params(("community_id" = Uuid, Path, description = "Community ID")),
    responses(
        (status = 200, description = "Community found", body = CommunityResponse),
        (status = 404, description = "Community not found")
    ),
    tag = "Policy"
)]
#[get("/policy/network/communities/{community_id}")]
pub(crate) async fn get_community(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let community_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::community::GET,
            authz::actions::resource_kinds::COMMUNITY,
            community_id.to_string(),
        )
        .build(),
    )
    .await?;
    let item = community_service::get_community(state.storage.communities(), community_id).await?;
    Ok(HttpResponse::Ok().json(CommunityResponse::from_domain(&item)))
}

/// Delete a community
#[utoipa::path(
    delete,
    path = "/api/v1/policy/network/communities/{community_id}",
    params(("community_id" = Uuid, Path, description = "Community ID")),
    responses(
        (status = 204, description = "Community deleted"),
        (status = 404, description = "Community not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/network/communities/{community_id}")]
pub(crate) async fn delete_community(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let community_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::community::DELETE,
            authz::actions::resource_kinds::COMMUNITY,
            community_id.to_string(),
        )
        .build(),
    )
    .await?;
    community_service::delete_community(
        state.storage.communities(),
        state.storage.audit(),
        &state.events,
        community_id,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_group_policy_community_and_mapping() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        // Create a network
        let net_req = test::TestRequest::post()
            .uri("/inventory/networks")
            .set_json(serde_json::json!({"cidr": "172.30.0.0/24", "description": "comm-test"}))
            .to_request();
        let resp = test::call_service(&app, net_req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Create a network policy
        let policy_req = test::TestRequest::post()
            .uri("/policy/network/policies")
            .set_json(serde_json::json!({"name": "comm-policy", "description": "test"}))
            .to_request();
        let resp = test::call_service(&app, policy_req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Create a community
        let comm_req = test::TestRequest::post()
            .uri("/policy/network/communities")
            .set_json(serde_json::json!({
                "policy_name": "comm-policy",
                "network": "172.30.0.0/24",
                "name": "test-community",
                "description": "a test community"
            }))
            .to_request();
        let response = test::call_service(&app, comm_req).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let created: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(created["name"], "test-community");
        assert_eq!(created["policy_name"], "comm-policy");
    }
}
