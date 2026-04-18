use std::collections::HashMap;

use actix_web::{HttpRequest, HttpResponse, get, web};
use serde::Deserialize;

use crate::{
    AppState,
    authz::actions,
    domain::{
        filters::RecordFilter,
        pagination::{PageRequest, SortDirection},
    },
    errors::AppError,
};

use super::SystemListResponse;
use super::authz::{request as authz_request, require};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(record_types)
        .service(rrsets)
        .service(list_records_endpoint);
}

/// List record types
#[utoipa::path(
    get,
    path = "/api/v1/dns/record-types",
    responses(
        (status = 200, description = "List of record types", body = SystemListResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/record-types")]
pub(crate) async fn record_types(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            actions::record_type::LIST,
            actions::resource_kinds::RECORD_TYPE,
            "*",
        ),
    )
    .await?;
    let page = state
        .services
        .records()
        .list_types(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.reader.backend_kind(),
    )))
}

/// List RRsets
#[utoipa::path(
    get,
    path = "/api/v1/dns/rrsets",
    responses(
        (status = 200, description = "List of RRsets", body = SystemListResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/rrsets")]
pub(crate) async fn rrsets(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            actions::rrset::LIST,
            actions::resource_kinds::RRSET,
            "*",
        ),
    )
    .await?;
    let page = state
        .services
        .records()
        .list_rrsets(&PageRequest::default())
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        page,
        state.reader.backend_kind(),
    )))
}

#[derive(Deserialize)]
pub struct ListRecordsQuery {
    // Pagination + sort
    after: Option<uuid::Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    // Operator-based filter params
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl ListRecordsQuery {
    fn into_parts(self) -> Result<(PageRequest, RecordFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let filter = RecordFilter::from_query_params(self.filters)?;
        Ok((page, filter))
    }
}

/// List records with optional filters
#[utoipa::path(
    get,
    path = "/api/v1/dns/records",
    responses(
        (status = 200, description = "List of records", body = SystemListResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/records")]
pub(crate) async fn list_records_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<ListRecordsQuery>,
) -> Result<HttpResponse, AppError> {
    require(
        &state,
        authz_request(
            &req,
            actions::record::LIST,
            actions::resource_kinds::RECORD,
            "*",
        ),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state
        .services
        .records()
        .list_records(&page, &filter)
        .await?;
    Ok(HttpResponse::Ok().json(SystemListResponse::from_page(
        result,
        state.reader.backend_kind(),
    )))
}
