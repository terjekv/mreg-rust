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
        attachment::{AttachmentCommunityAssignment, CreateAttachmentCommunityAssignment},
        filters::AttachmentCommunityAssignmentFilter,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
};

use super::authz::{request as authz_request, require};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_attachment_community_assignments)
        .service(create_attachment_community_assignment)
        .service(get_attachment_community_assignment)
        .service(delete_attachment_community_assignment);
}

#[derive(Deserialize)]
pub struct AttachmentCommunityAssignmentQuery {
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

impl AttachmentCommunityAssignmentQuery {
    fn into_parts(self) -> Result<(PageRequest, AttachmentCommunityAssignmentFilter), AppError> {
        Ok((
            PageRequest {
                after: self.after,
                limit: self.limit,
                sort_by: self.sort_by,
                sort_dir: self.sort_dir,
            },
            AttachmentCommunityAssignmentFilter::from_query_params(self.filters)?,
        ))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAttachmentCommunityAssignmentRequest {
    attachment_id: Uuid,
    policy_name: String,
    community_name: String,
}

impl CreateAttachmentCommunityAssignmentRequest {
    fn into_command(self) -> Result<CreateAttachmentCommunityAssignment, AppError> {
        Ok(CreateAttachmentCommunityAssignment::new(
            self.attachment_id,
            NetworkPolicyName::new(self.policy_name)?,
            CommunityName::new(self.community_name)?,
        ))
    }
}

#[derive(Clone, Serialize, ToSchema)]
pub struct AttachmentCommunityAssignmentResponse {
    id: Uuid,
    attachment_id: Uuid,
    host_id: Uuid,
    host_name: String,
    network_id: Uuid,
    network: String,
    community_id: Uuid,
    community_name: String,
    policy_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttachmentCommunityAssignmentResponse {
    pub fn from_domain(value: &AttachmentCommunityAssignment) -> Self {
        Self {
            id: value.id(),
            attachment_id: value.attachment_id(),
            host_id: value.host_id(),
            host_name: value.host_name().as_str().to_string(),
            network_id: value.network_id(),
            network: value.network_cidr().as_str(),
            community_id: value.community_id(),
            community_name: value.community_name().as_str().to_string(),
            policy_name: value.policy_name().as_str().to_string(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

fn assignment_authorization(
    req: &HttpRequest,
    action: &str,
    assignment: &AttachmentCommunityAssignment,
) -> crate::authz::AuthorizationRequestBuilder {
    authz_request(
        req,
        action,
        authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
        assignment.id().to_string(),
    )
    .attr(
        "attachment_id",
        AttrValue::String(assignment.attachment_id().to_string()),
    )
    .attr(
        "host_name",
        AttrValue::String(assignment.host_name().as_str().to_string()),
    )
    .attr("network", AttrValue::Ip(assignment.network_cidr().as_str()))
    .attr(
        "policy_name",
        AttrValue::String(assignment.policy_name().as_str().to_string()),
    )
    .attr(
        "community_name",
        AttrValue::String(assignment.community_name().as_str().to_string()),
    )
}

#[get("/policy/network/attachment-community-assignments")]
pub(crate) async fn list_attachment_community_assignments(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<AttachmentCommunityAssignmentQuery>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            authz::actions::attachment_community_assignment::LIST,
            authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
            "*",
        ),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state
        .services
        .attachments()
        .list_attachment_community_assignments(&page, &filter)
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        AttachmentCommunityAssignmentResponse::from_domain,
    )))
}

#[post("/policy/network/attachment-community-assignments")]
pub(crate) async fn create_attachment_community_assignment(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateAttachmentCommunityAssignmentRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let attachment = state
        .services
        .attachments()
        .get_attachment(request.attachment_id)
        .await?;
    let policy_name = NetworkPolicyName::new(&request.policy_name)?;
    let community_name = CommunityName::new(&request.community_name)?;
    let community = state
        .services
        .communities()
        .find_by_names(&policy_name, &community_name)
        .await?;
    if community.network_cidr() != attachment.network_cidr() {
        return Err(AppError::validation(
            "community must belong to the attachment network",
        ));
    }
    require(
        &state,
        authz_request(
            &req,
            authz::actions::attachment_community_assignment::CREATE,
            authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
            request.attachment_id.to_string(),
        )
        .attr(
            "attachment_id",
            AttrValue::String(attachment.id().to_string()),
        )
        .attr(
            "host_name",
            AttrValue::String(attachment.host_name().as_str().to_string()),
        )
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
    let assignment = state
        .services
        .attachments()
        .create_attachment_community_assignment(request.into_command()?)
        .await?;
    Ok(
        HttpResponse::Created().json(AttachmentCommunityAssignmentResponse::from_domain(
            &assignment,
        )),
    )
}

#[get("/policy/network/attachment-community-assignments/{assignment_id}")]
pub(crate) async fn get_attachment_community_assignment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let assignment_id = path.into_inner();
    let assignment = state
        .services
        .attachments()
        .get_attachment_community_assignment(assignment_id)
        .await?;
    require(
        &state,
        assignment_authorization(
            &req,
            authz::actions::attachment_community_assignment::GET,
            &assignment,
        ),
    )
    .await?;
    Ok(
        HttpResponse::Ok().json(AttachmentCommunityAssignmentResponse::from_domain(
            &assignment,
        )),
    )
}

#[delete("/policy/network/attachment-community-assignments/{assignment_id}")]
pub(crate) async fn delete_attachment_community_assignment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let assignment_id = path.into_inner();
    let assignment = state
        .services
        .attachments()
        .get_attachment_community_assignment(assignment_id)
        .await?;
    require(
        &state,
        assignment_authorization(
            &req,
            authz::actions::attachment_community_assignment::DELETE,
            &assignment,
        ),
    )
    .await?;
    state
        .services
        .attachments()
        .delete_attachment_community_assignment(assignment_id)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}
