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
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::HostCommunityAssignment,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{CommunityName, Hostname, IpAddressValue, NetworkPolicyName},
    },
    errors::AppError,
};

use super::authz::{request as authz_request, require};

crate::page_response!(
    HostCommunityAssignmentPageResponse,
    HostCommunityAssignmentResponse,
    "Paginated list of host-community assignments."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_host_community_assignments)
        .service(create_host_community_assignment)
        .service(get_host_community_assignment)
        .service(delete_host_community_assignment);
}

#[derive(Deserialize)]
pub struct HostCommunityAssignmentQuery {
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

impl HostCommunityAssignmentQuery {
    fn into_parts(self) -> Result<(PageRequest, HostCommunityAssignmentFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let filter = HostCommunityAssignmentFilter::from_query_params(self.filters)?;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateHostCommunityAssignmentRequest {
    host_name: String,
    address: String,
    policy_name: String,
    community_name: String,
}

impl CreateHostCommunityAssignmentRequest {
    fn into_command(
        self,
    ) -> Result<crate::domain::host_community_assignment::CreateHostCommunityAssignment, AppError>
    {
        Ok(
            crate::domain::host_community_assignment::CreateHostCommunityAssignment::new(
                Hostname::new(self.host_name)?,
                IpAddressValue::new(self.address)?,
                NetworkPolicyName::new(self.policy_name)?,
                CommunityName::new(self.community_name)?,
            ),
        )
    }
}

#[derive(Serialize, ToSchema)]
pub struct HostCommunityAssignmentResponse {
    id: Uuid,
    host_id: Uuid,
    host_name: String,
    ip_address_id: Uuid,
    address: String,
    community_id: Uuid,
    community_name: String,
    policy_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostCommunityAssignmentResponse {
    fn from_domain(value: &HostCommunityAssignment) -> Self {
        Self {
            id: value.id(),
            host_id: value.host_id(),
            host_name: value.host_name().as_str().to_string(),
            ip_address_id: value.ip_address_id(),
            address: value.address().as_str(),
            community_id: value.community_id(),
            community_name: value.community_name().as_str().to_string(),
            policy_name: value.policy_name().as_str().to_string(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

async fn assignment_authorization(
    state: &AppState,
    req: &HttpRequest,
    action: &str,
    assignment: &HostCommunityAssignment,
) -> Result<crate::authz::AuthorizationRequestBuilder, AppError> {
    let ip = state
        .services
        .hosts()
        .get_ip_address(assignment.address())
        .await?;
    let attachment = state
        .services
        .attachments()
        .get_attachment(ip.attachment_id())
        .await?;
    Ok(authz_request(
        req,
        action,
        authz::actions::resource_kinds::HOST_COMMUNITY_ASSIGNMENT,
        assignment.id().to_string(),
    )
    .attr(
        "host_name",
        AttrValue::String(assignment.host_name().as_str().to_string()),
    )
    .attr("address", AttrValue::Ip(assignment.address().as_str()))
    .attr("network", AttrValue::Ip(attachment.network_cidr().as_str()))
    .attr(
        "policy_name",
        AttrValue::String(assignment.policy_name().as_str().to_string()),
    )
    .attr(
        "community_name",
        AttrValue::String(assignment.community_name().as_str().to_string()),
    ))
}

/// List host-community assignments
#[utoipa::path(
    get,
    path = "/api/v1/policy/network/host-community-assignments",
    responses(
        (status = 200, description = "Paginated list of assignments", body = HostCommunityAssignmentPageResponse)
    ),
    tag = "Policy"
)]
#[get("/policy/network/host-community-assignments")]
pub(crate) async fn list_host_community_assignments(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<HostCommunityAssignmentQuery>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            authz::actions::host_community_assignment::LIST,
            authz::actions::resource_kinds::HOST_COMMUNITY_ASSIGNMENT,
            "*",
        ),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state
        .services
        .host_community_assignments()
        .list(&page, &filter)
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        HostCommunityAssignmentResponse::from_domain,
    )))
}

/// Create a host-community assignment
#[utoipa::path(
    post,
    path = "/api/v1/policy/network/host-community-assignments",
    request_body = CreateHostCommunityAssignmentRequest,
    responses(
        (status = 201, description = "Assignment created", body = HostCommunityAssignmentResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Assignment already exists")
    ),
    tag = "Policy"
)]
#[post("/policy/network/host-community-assignments")]
pub(crate) async fn create_host_community_assignment(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateHostCommunityAssignmentRequest>,
) -> Result<HttpResponse, AppError> {
    let command = payload.into_inner().into_command()?;
    let ip = state
        .services
        .hosts()
        .get_ip_address(command.address())
        .await?;
    let attachment = state
        .services
        .attachments()
        .get_attachment(ip.attachment_id())
        .await?;
    if attachment.host_name() != command.host_name() {
        return Err(AppError::validation(
            "host community assignment address must belong to the supplied host",
        ));
    }
    let community = state
        .services
        .communities()
        .find_by_names(command.policy_name(), command.community_name())
        .await?;
    if community.network_cidr() != attachment.network_cidr() {
        return Err(AppError::validation(
            "community must belong to the IP assignment network",
        ));
    }
    require(
        &state,
        authz_request(
            &req,
            authz::actions::host_community_assignment::CREATE,
            authz::actions::resource_kinds::HOST_COMMUNITY_ASSIGNMENT,
            format!(
                "{}:{}",
                command.host_name().as_str(),
                command.address().as_str()
            ),
        )
        .attr(
            "host_name",
            AttrValue::String(attachment.host_name().as_str().to_string()),
        )
        .attr("address", AttrValue::Ip(command.address().as_str()))
        .attr("network", AttrValue::Ip(attachment.network_cidr().as_str()))
        .attr(
            "policy_name",
            AttrValue::String(community.policy_name().as_str().to_string()),
        )
        .attr(
            "community_name",
            AttrValue::String(community.name().as_str().to_string()),
        ),
    )
    .await?;
    let item = state
        .services
        .host_community_assignments()
        .create(command)
        .await?;
    Ok(HttpResponse::Created().json(HostCommunityAssignmentResponse::from_domain(&item)))
}

/// Get a host-community assignment by ID
#[utoipa::path(
    get,
    path = "/api/v1/policy/network/host-community-assignments/{mapping_id}",
    params(("mapping_id" = Uuid, Path, description = "Mapping ID")),
    responses(
        (status = 200, description = "Assignment found", body = HostCommunityAssignmentResponse),
        (status = 404, description = "Assignment not found")
    ),
    tag = "Policy"
)]
#[get("/policy/network/host-community-assignments/{mapping_id}")]
pub(crate) async fn get_host_community_assignment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let mapping_id = path.into_inner();
    let item = state
        .services
        .host_community_assignments()
        .get(mapping_id)
        .await?;
    require(
        &state,
        assignment_authorization(
            state.get_ref(),
            &req,
            authz::actions::host_community_assignment::GET,
            &item,
        )
        .await?,
    )
    .await?;
    Ok(HttpResponse::Ok().json(HostCommunityAssignmentResponse::from_domain(&item)))
}

/// Delete a host-community assignment
#[utoipa::path(
    delete,
    path = "/api/v1/policy/network/host-community-assignments/{mapping_id}",
    params(("mapping_id" = Uuid, Path, description = "Mapping ID")),
    responses(
        (status = 204, description = "Assignment deleted"),
        (status = 404, description = "Assignment not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/network/host-community-assignments/{mapping_id}")]
pub(crate) async fn delete_host_community_assignment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let mapping_id = path.into_inner();
    let item = state
        .services
        .host_community_assignments()
        .get(mapping_id)
        .await?;
    require(
        &state,
        assignment_authorization(
            state.get_ref(),
            &req,
            authz::actions::host_community_assignment::DELETE,
            &item,
        )
        .await?,
    )
    .await?;
    state
        .services
        .host_community_assignments()
        .delete(mapping_id)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}
