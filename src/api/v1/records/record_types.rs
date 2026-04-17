use actix_web::{HttpRequest, HttpResponse, delete, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, require_permission},
    domain::{
        resource_records::{
            CreateRecordTypeDefinition, RecordCardinality, RecordFieldKind, RecordFieldSchema,
            RecordOwnerKind, RecordRfcProfile, RecordTypeDefinition, RecordTypeSchema,
        },
        types::{DnsTypeCode, RecordTypeName},
    },
    errors::AppError,
};

use crate::api::v1::authz::request as authz_request;

#[derive(Deserialize, ToSchema)]
pub struct CreateRecordFieldSchemaRequest {
    name: String,
    kind: RecordFieldKind,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    repeated: bool,
    #[serde(default)]
    options: Vec<String>,
}

impl CreateRecordFieldSchemaRequest {
    pub(super) fn into_domain(self) -> Result<RecordFieldSchema, AppError> {
        RecordFieldSchema::new(
            self.name,
            self.kind,
            self.required,
            self.repeated,
            self.options,
        )
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateRecordTypeRequest {
    name: String,
    dns_type: Option<i32>,
    owner_kind: RecordOwnerKind,
    cardinality: RecordCardinality,
    #[serde(default)]
    zone_bound: bool,
    fields: Vec<CreateRecordFieldSchemaRequest>,
    #[serde(default)]
    #[schema(value_type = Object)]
    behavior_flags: Value,
    render_template: Option<String>,
}

impl CreateRecordTypeRequest {
    fn into_command(self) -> Result<CreateRecordTypeDefinition, AppError> {
        let fields = self
            .fields
            .into_iter()
            .map(CreateRecordFieldSchemaRequest::into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CreateRecordTypeDefinition::new(
            RecordTypeName::new(self.name)?,
            self.dns_type.map(DnsTypeCode::new).transpose()?,
            RecordTypeSchema::new(
                self.owner_kind,
                self.cardinality,
                self.zone_bound,
                fields,
                self.behavior_flags,
                self.render_template,
            )?,
            false,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct RecordTypeResponse {
    id: Uuid,
    name: String,
    dns_type: Option<i32>,
    built_in: bool,
    rfc_profile: Option<RecordRfcProfile>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RecordTypeResponse {
    pub(crate) fn from_domain(record_type: &RecordTypeDefinition) -> Self {
        Self {
            id: record_type.id(),
            name: record_type.name().as_str().to_string(),
            dns_type: record_type.dns_type().map(|v| v.as_i32()),
            built_in: record_type.built_in(),
            rfc_profile: record_type.schema().rfc_profile().ok().flatten(),
            created_at: record_type.created_at(),
            updated_at: record_type.updated_at(),
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_record_type)
        .service(delete_record_type_endpoint);
}

/// Create a new record type
#[utoipa::path(
    post,
    path = "/api/v1/dns/record-types",
    request_body = CreateRecordTypeRequest,
    responses(
        (status = 201, description = "Record type created", body = RecordTypeResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Record type already exists")
    ),
    tag = "DNS"
)]
#[post("/dns/record-types")]
pub(crate) async fn create_record_type(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateRecordTypeRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::record_type::CREATE,
            authz::actions::resource_kinds::RECORD_TYPE,
            request.name.clone(),
        )
        .build(),
    )
    .await?;
    let record_type = state
        .services
        .records()
        .create_type(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(RecordTypeResponse::from_domain(&record_type)))
}

/// Delete a record type
#[utoipa::path(
    delete,
    path = "/api/v1/dns/record-types/{name}",
    params(("name" = String, Path, description = "Record type name")),
    responses(
        (status = 204, description = "Record type deleted"),
        (status = 404, description = "Record type not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/record-types/{name}")]
pub(crate) async fn delete_record_type_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = RecordTypeName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::record_type::DELETE,
            authz::actions::resource_kinds::RECORD_TYPE,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    state.services.records().delete_record_type(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}
