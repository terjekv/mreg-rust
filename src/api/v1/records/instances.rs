use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission, require_permissions},
    domain::{
        resource_records::{
            CreateRecordInstance, RawRdataValue, RecordInstance, RecordOwnerKind, UpdateRecord,
        },
        types::{RecordTypeName, Ttl, UpdateField},
    },
    errors::AppError,
};

use crate::api::v1::authz::request as authz_request;

#[derive(Deserialize, ToSchema)]
pub struct CreateRecordRequest {
    type_name: String,
    owner_kind: Option<RecordOwnerKind>,
    owner_name: String,
    anchor_name: Option<String>,
    ttl: Option<u32>,
    #[schema(value_type = Option<Object>)]
    data: Option<Value>,
    raw_rdata: Option<String>,
}

impl CreateRecordRequest {
    fn into_command(self) -> Result<CreateRecordInstance, AppError> {
        CreateRecordInstance::with_reference(
            RecordTypeName::new(self.type_name)?,
            self.owner_kind,
            self.owner_name,
            self.anchor_name,
            self.ttl.map(Ttl::new).transpose()?,
            self.data,
            self.raw_rdata
                .map(RawRdataValue::from_presentation)
                .transpose()?,
        )
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateRecordRequest {
    #[serde(default)]
    #[schema(value_type = Option<u32>)]
    ttl: UpdateField<u32>,
    #[schema(value_type = Option<Object>)]
    data: Option<Value>,
    raw_rdata: Option<String>,
}

impl UpdateRecordRequest {
    fn into_command(self) -> Result<UpdateRecord, AppError> {
        UpdateRecord::new(
            self.ttl.try_map(Ttl::new)?,
            self.data,
            self.raw_rdata
                .map(RawRdataValue::from_presentation)
                .transpose()?,
        )
    }
}

#[derive(Serialize, ToSchema)]
pub struct RecordResponse {
    id: Uuid,
    rrset_id: Uuid,
    type_id: Uuid,
    type_name: String,
    owner_kind: Option<RecordOwnerKind>,
    owner_name: String,
    ttl: Option<u32>,
    #[schema(value_type = Object)]
    data: Value,
    raw_rdata: Option<String>,
    rendered: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RecordResponse {
    pub(crate) fn from_domain(record: &RecordInstance) -> Self {
        Self {
            id: record.id(),
            rrset_id: record.rrset_id(),
            type_id: record.type_id(),
            type_name: record.type_name().as_str().to_string(),
            owner_kind: record.owner_kind().cloned(),
            owner_name: record.owner_name().to_string(),
            ttl: record.ttl().map(|ttl| ttl.as_u32()),
            data: record.data().clone(),
            raw_rdata: record.raw_rdata().map(RawRdataValue::presentation),
            rendered: record.rendered().map(str::to_string),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_record)
        .service(get_record_endpoint)
        .service(update_record_endpoint)
        .service(delete_record_endpoint);
}

/// Create a new record
#[utoipa::path(
    post,
    path = "/api/v1/dns/records",
    request_body = CreateRecordRequest,
    responses(
        (status = 201, description = "Record created", body = RecordResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Duplicate record")
    ),
    tag = "DNS"
)]
#[post("/dns/records")]
pub(crate) async fn create_record(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateRecordRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let action = if request.anchor_name.is_some() {
        authz::actions::record::CREATE_ANCHORED
    } else {
        authz::actions::record::CREATE_UNANCHORED
    };
    let mut authz = authz_request(
        &req,
        action,
        authz::actions::resource_kinds::RECORD,
        request.owner_name.clone(),
    )
    .attr("type_name", AttrValue::String(request.type_name.clone()))
    .attr("owner_name", AttrValue::String(request.owner_name.clone()));
    if let Some(owner_kind) = request.owner_kind.as_ref() {
        authz = authz.attr(
            "owner_kind",
            AttrValue::String(
                serde_json::to_value(owner_kind)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("{owner_kind:?}").to_lowercase()),
            ),
        );
    }
    if let Some(anchor_name) = &request.anchor_name {
        authz = authz.attr("anchor_name", AttrValue::String(anchor_name.clone()));
    }
    if let Some(ttl) = request.ttl {
        authz = authz.attr("ttl", AttrValue::Long(i64::from(ttl)));
    }
    require_permission(&state.authz, authz.build()).await?;
    let record = state
        .services
        .records()
        .create_record(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(RecordResponse::from_domain(&record)))
}

/// Get a record by ID
#[utoipa::path(
    get,
    path = "/api/v1/dns/records/{id}",
    params(("id" = Uuid, Path, description = "Record ID")),
    responses(
        (status = 200, description = "Record found", body = RecordResponse),
        (status = 404, description = "Record not found")
    ),
    tag = "DNS"
)]
#[get("/dns/records/{id}")]
pub(crate) async fn get_record_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::record::GET,
            authz::actions::resource_kinds::RECORD,
            record_id.to_string(),
        )
        .build(),
    )
    .await?;
    let record = state.services.records().get_record(record_id).await?;
    Ok(HttpResponse::Ok().json(RecordResponse::from_domain(&record)))
}

/// Update a record
#[utoipa::path(
    patch,
    path = "/api/v1/dns/records/{id}",
    params(("id" = Uuid, Path, description = "Record ID")),
    request_body = UpdateRecordRequest,
    responses(
        (status = 200, description = "Record updated", body = RecordResponse),
        (status = 404, description = "Record not found")
    ),
    tag = "DNS"
)]
#[patch("/dns/records/{id}")]
pub(crate) async fn update_record_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    payload: web::Json<UpdateRecordRequest>,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    let request = payload.into_inner();
    let mut authz_requests = Vec::new();
    if request.ttl.is_changed() {
        let mut authz = authz_request(
            &req,
            authz::actions::record::UPDATE_TTL,
            authz::actions::resource_kinds::RECORD,
            record_id.to_string(),
        );
        match &request.ttl {
            UpdateField::Set(ttl) => {
                authz = authz.attr("new_ttl", AttrValue::Long(i64::from(*ttl)));
            }
            UpdateField::Clear => {
                authz = authz.attr("clear_ttl", AttrValue::Bool(true));
            }
            UpdateField::Unchanged => {}
        }
        authz_requests.push(authz.build());
    }
    if request.data.is_some() || request.raw_rdata.is_some() {
        authz_requests.push(
            authz_request(
                &req,
                authz::actions::record::UPDATE_DATA,
                authz::actions::resource_kinds::RECORD,
                record_id.to_string(),
            )
            .build(),
        );
    }
    require_permissions(&state.authz, authz_requests).await?;
    let record = state
        .services
        .records()
        .update_record(record_id, request.into_command()?)
        .await?;
    Ok(HttpResponse::Ok().json(RecordResponse::from_domain(&record)))
}

/// Delete a record
#[utoipa::path(
    delete,
    path = "/api/v1/dns/records/{id}",
    params(("id" = Uuid, Path, description = "Record ID")),
    responses(
        (status = 204, description = "Record deleted"),
        (status = 404, description = "Record not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/records/{id}")]
pub(crate) async fn delete_record_endpoint(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::record::DELETE,
            authz::actions::resource_kinds::RECORD,
            record_id.to_string(),
        )
        .build(),
    )
    .await?;
    state.services.records().delete_record(record_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn built_in_cname_record_can_be_created_for_host() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({
                    "name": "app.example.org",
                    "comment": "App host"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "CNAME",
                    "owner_kind": "host",
                    "owner_name": "app.example.org",
                    "data": {
                        "target": "alias.example.org."
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["data"]["target"], "alias.example.org");
    }

    #[actix_web::test]
    async fn cname_owner_rejects_other_record_types() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({
                    "name": "api.example.org",
                    "comment": "API host"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "CNAME",
                    "owner_kind": "host",
                    "owner_name": "api.example.org",
                    "ttl": 300,
                    "data": {
                        "target": "alias.example.org"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "TXT",
                    "owner_kind": "host",
                    "owner_name": "api.example.org",
                    "ttl": 300,
                    "data": {
                        "value": "hello"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[actix_web::test]
    async fn mx_rrset_requires_matching_ttl() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/nameservers")
                .set_json(serde_json::json!({
                    "name": "ns1.example.org"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/forward-zones")
                .set_json(serde_json::json!({
                    "name": "example.org",
                    "primary_ns": "ns1.example.org",
                    "nameservers": ["ns1.example.org"],
                    "email": "hostmaster@example.org"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "MX",
                    "owner_kind": "forward_zone",
                    "owner_name": "example.org",
                    "ttl": 3600,
                    "data": {
                        "preference": 10,
                        "exchange": "mail.example.org"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "MX",
                    "owner_kind": "forward_zone",
                    "owner_name": "example.org",
                    "ttl": 7200,
                    "data": {
                        "preference": 20,
                        "exchange": "backup.example.org"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn mx_exchange_cannot_reference_alias_owner() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        for request in [
            test::TestRequest::post()
                .uri("/dns/nameservers")
                .set_json(serde_json::json!({"name": "ns1.example.org"}))
                .to_request(),
            test::TestRequest::post()
                .uri("/dns/forward-zones")
                .set_json(serde_json::json!({
                    "name": "example.org",
                    "primary_ns": "ns1.example.org",
                    "nameservers": ["ns1.example.org"],
                    "email": "hostmaster@example.org"
                }))
                .to_request(),
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({
                    "name": "mailalias.example.org",
                    "zone": "example.org",
                    "comment": "mail alias"
                }))
                .to_request(),
        ] {
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "CNAME",
                    "owner_kind": "host",
                    "owner_name": "mailalias.example.org",
                    "ttl": 300,
                    "data": {
                        "target": "realmail.example.org"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "MX",
                    "owner_kind": "forward_zone",
                    "owner_name": "example.org",
                    "ttl": 300,
                    "data": {
                        "preference": 10,
                        "exchange": "mailalias.example.org"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[actix_web::test]
    async fn zone_serial_is_bumped_on_record_create_and_delete() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        // Create nameserver and zone
        for request in [
            test::TestRequest::post()
                .uri("/dns/nameservers")
                .set_json(serde_json::json!({"name": "ns1.serial.org"}))
                .to_request(),
            test::TestRequest::post()
                .uri("/dns/forward-zones")
                .set_json(serde_json::json!({
                    "name": "serial.org",
                    "primary_ns": "ns1.serial.org",
                    "nameservers": ["ns1.serial.org"],
                    "email": "hostmaster@serial.org"
                }))
                .to_request(),
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({
                    "name": "www.serial.org",
                    "zone": "serial.org",
                    "comment": "serial test"
                }))
                .to_request(),
        ] {
            let response = test::call_service(&app, request).await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        // Read initial serial
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/dns/forward-zones/serial.org")
                .to_request(),
        )
        .await;
        let body: serde_json::Value = test::read_body_json(response).await;
        let initial_serial = body["serial_no"].as_u64().expect("serial_no");

        // Create a record in the zone
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "CNAME",
                    "owner_kind": "host",
                    "owner_name": "www.serial.org",
                    "data": { "target": "web.serial.org" }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let record_body: serde_json::Value = test::read_body_json(response).await;
        let record_id = record_body["id"].as_str().expect("record id");

        // Serial should have bumped
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/dns/forward-zones/serial.org")
                .to_request(),
        )
        .await;
        let body: serde_json::Value = test::read_body_json(response).await;
        let after_create_serial = body["serial_no"].as_u64().expect("serial_no");
        assert!(
            after_create_serial > initial_serial,
            "serial should increase after record creation: {} > {}",
            after_create_serial,
            initial_serial
        );

        // Delete the record
        let response = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/dns/records/{record_id}"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Serial should have bumped again
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/dns/forward-zones/serial.org")
                .to_request(),
        )
        .await;
        let body: serde_json::Value = test::read_body_json(response).await;
        let after_delete_serial = body["serial_no"].as_u64().expect("serial_no");
        assert!(
            after_delete_serial > after_create_serial,
            "serial should increase after record deletion: {} > {}",
            after_delete_serial,
            after_create_serial
        );
    }

    #[actix_web::test]
    async fn delete_record_removes_record_and_empty_rrset() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        // Create host and record
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/hosts")
                .set_json(serde_json::json!({
                    "name": "del.example.org",
                    "comment": "deletion test"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "CNAME",
                    "owner_kind": "host",
                    "owner_name": "del.example.org",
                    "data": { "target": "target.example.org" }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value = test::read_body_json(response).await;
        let record_id = body["id"].as_str().expect("record id");
        let rrset_id = body["rrset_id"].as_str().expect("rrset id");

        // GET record works
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/dns/records/{record_id}"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        // DELETE record
        let response = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri(&format!("/dns/records/{record_id}"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // GET record now returns 404
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/dns/records/{record_id}"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // RRset should also be gone (was the only record)
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/dns/rrsets/{rrset_id}"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn unanchored_srv_and_raw_rfc3597_records_are_supported() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/record-types")
                .set_json(serde_json::json!({
                    "name": "TYPE65400",
                    "dns_type": 65400,
                    "owner_kind": "host",
                    "cardinality": "multiple",
                    "fields": [],
                    "behavior_flags": {
                        "rfc3597": {
                            "allow_raw_rdata": true
                        }
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "SRV",
                    "owner_name": "_sip._tcp.example.org",
                    "ttl": 300,
                    "data": {
                        "priority": 10,
                        "weight": 5,
                        "port": 5060,
                        "target": "sip1.example.org"
                    }
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["owner_name"], "_sip._tcp.example.org");
        assert!(body["owner_kind"].is_null());

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/dns/records")
                .set_json(serde_json::json!({
                    "type_name": "TYPE65400",
                    "owner_name": "opaque.example.org",
                    "ttl": 600,
                    "raw_rdata": "\\# 4 deadbeef"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["raw_rdata"], "\\# 4 deadbeef");
        assert!(body["data"].is_null());

        let response = test::call_service(
            &app,
            test::TestRequest::get().uri("/dns/rrsets").to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert!(
            body["items"]
                .as_array()
                .expect("rrset list")
                .iter()
                .any(|item| item["owner_name"] == "_sip._tcp.example.org")
        );
    }
}
