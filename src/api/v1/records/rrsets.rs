use actix_web::{HttpRequest, HttpResponse, delete, get, web};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, require_permission},
    domain::resource_records::{RecordOwnerKind, RecordRrset},
    errors::AppError,
    services::records as record_service,
};

use crate::api::v1::authz::request as authz_request;

#[derive(Serialize, ToSchema)]
pub struct RrsetResponse {
    id: Uuid,
    type_id: Uuid,
    type_name: String,
    dns_class: String,
    owner_name: String,
    anchor_kind: Option<RecordOwnerKind>,
    anchor_id: Option<Uuid>,
    anchor_name: Option<String>,
    zone_id: Option<Uuid>,
    ttl: Option<u32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RrsetResponse {
    pub(crate) fn from_domain(rrset: &RecordRrset) -> Self {
        Self {
            id: rrset.id(),
            type_id: rrset.type_id(),
            type_name: rrset.type_name().as_str().to_string(),
            dns_class: serde_json::to_value(rrset.dns_class())
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| format!("{:?}", rrset.dns_class())),
            owner_name: rrset.owner_name().as_str().to_string(),
            anchor_kind: rrset.anchor_kind().cloned(),
            anchor_id: rrset.anchor_id(),
            anchor_name: rrset.anchor_name().map(str::to_string),
            zone_id: rrset.zone_id(),
            ttl: rrset.ttl().map(|t| t.as_u32()),
            created_at: rrset.created_at(),
            updated_at: rrset.updated_at(),
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(get_rrset_endpoint)
        .service(delete_rrset_endpoint);
}

/// Get an RRset by ID
#[utoipa::path(
    get,
    path = "/api/v1/dns/rrsets/{id}",
    params(("id" = Uuid, Path, description = "RRset ID")),
    responses(
        (status = 200, description = "RRset found", body = RrsetResponse),
        (status = 404, description = "RRset not found")
    ),
    tag = "DNS"
)]
#[get("/dns/rrsets/{id}")]
pub(crate) async fn get_rrset_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let rrset_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::rrset::GET,
            authz::actions::resource_kinds::RRSET,
            rrset_id.to_string(),
        )
        .build(),
    )
    .await?;
    let rrset = record_service::get_rrset(state.storage.records(), rrset_id).await?;
    Ok(HttpResponse::Ok().json(RrsetResponse::from_domain(&rrset)))
}

/// Delete an RRset
#[utoipa::path(
    delete,
    path = "/api/v1/dns/rrsets/{id}",
    params(("id" = Uuid, Path, description = "RRset ID")),
    responses(
        (status = 204, description = "RRset deleted"),
        (status = 404, description = "RRset not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/rrsets/{id}")]
pub(crate) async fn delete_rrset_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let rrset_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::rrset::DELETE,
            authz::actions::resource_kinds::RRSET,
            rrset_id.to_string(),
        )
        .build(),
    )
    .await?;
    record_service::delete_rrset(
        state.storage.records(),
        state.storage.audit(),
        &state.events,
        rrset_id,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}
