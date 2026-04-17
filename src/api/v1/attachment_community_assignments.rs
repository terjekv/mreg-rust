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
        attachment::{AttachmentCommunityAssignment, CreateAttachmentCommunityAssignment},
        filters::AttachmentCommunityAssignmentFilter,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{CommunityName, NetworkPolicyName},
    },
    errors::AppError,
    services::attachments as attachment_service,
};

use super::authz::request as authz_request;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_attachment_community_assignments)
        .service(create_attachment_community_assignment)
        .service(get_attachment_community_assignment)
        .service(delete_attachment_community_assignment);
}

#[derive(Deserialize)]
pub struct AttachmentCommunityAssignmentQuery {
    after: Option<Uuid>,
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

#[get("/policy/network/attachment-community-assignments")]
pub(crate) async fn list_attachment_community_assignments(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<AttachmentCommunityAssignmentQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_community_assignment::LIST,
            authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state
        .storage
        .attachment_community_assignments()
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
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_community_assignment::CREATE,
            authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
            request.attachment_id.to_string(),
        )
        .attr(
            "attachment_id",
            AttrValue::String(request.attachment_id.to_string()),
        )
        .attr(
            "policy_name",
            AttrValue::String(request.policy_name.clone()),
        )
        .attr(
            "community_name",
            AttrValue::String(request.community_name.clone()),
        )
        .build(),
    )
    .await?;
    let assignment = attachment_service::create_attachment_community_assignment(
        state.storage.attachment_community_assignments(),
        request.into_command()?,
        state.storage.audit(),
        &state.events,
    )
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
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_community_assignment::GET,
            authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
            assignment_id.to_string(),
        )
        .build(),
    )
    .await?;
    let assignment = state
        .storage
        .attachment_community_assignments()
        .get_attachment_community_assignment(assignment_id)
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
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_community_assignment::DELETE,
            authz::actions::resource_kinds::ATTACHMENT_COMMUNITY_ASSIGNMENT,
            assignment_id.to_string(),
        )
        .build(),
    )
    .await?;
    attachment_service::delete_attachment_community_assignment(
        state.storage.attachment_community_assignments(),
        assignment_id,
        state.storage.audit(),
        &state.events,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
