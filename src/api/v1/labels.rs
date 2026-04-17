use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{PageRequest, PageResponse},
        types::LabelName,
    },
    errors::AppError,
};

use super::authz::request as authz_request;

crate::page_response!(
    LabelPageResponse,
    LabelResponse,
    "Paginated list of labels."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_labels)
        .service(create_label)
        .service(get_label)
        .service(update_label)
        .service(delete_label);
}

#[derive(Deserialize, ToSchema)]
pub struct CreateLabelRequest {
    name: String,
    description: String,
}

impl CreateLabelRequest {
    fn into_command(self) -> Result<CreateLabel, AppError> {
        CreateLabel::new(LabelName::new(self.name)?, self.description)
    }
}

#[derive(Serialize, ToSchema)]
pub struct LabelResponse {
    id: Uuid,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl LabelResponse {
    fn from_domain(label: &Label) -> Self {
        Self {
            id: label.id(),
            name: label.name().as_str().to_string(),
            description: label.description().to_string(),
            created_at: label.created_at(),
            updated_at: label.updated_at(),
        }
    }
}

/// List all labels
#[utoipa::path(
    get,
    path = "/api/v1/inventory/labels",
    params(PageRequest),
    responses(
        (status = 200, description = "Paginated list of labels", body = LabelPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/labels")]
pub(crate) async fn list_labels(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::label::LIST,
            authz::actions::resource_kinds::LABEL,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state.services.labels().list(&query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(page, LabelResponse::from_domain)))
}

/// Create a new label
#[utoipa::path(
    post,
    path = "/api/v1/inventory/labels",
    request_body = CreateLabelRequest,
    responses(
        (status = 201, description = "Label created", body = LabelResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Label already exists")
    ),
    tag = "Inventory"
)]
#[post("/inventory/labels")]
pub(crate) async fn create_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateLabelRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::label::CREATE,
            authz::actions::resource_kinds::LABEL,
            request.name.clone(),
        )
        .build(),
    )
    .await?;
    let label = state
        .services
        .labels()
        .create(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(LabelResponse::from_domain(&label)))
}

/// Get a label by name
#[utoipa::path(
    get,
    path = "/api/v1/inventory/labels/{name}",
    params(("name" = String, Path, description = "Label name")),
    responses(
        (status = 200, description = "Label found", body = LabelResponse),
        (status = 404, description = "Label not found")
    ),
    tag = "Inventory"
)]
#[get("/inventory/labels/{name}")]
pub(crate) async fn get_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = LabelName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::label::GET,
            authz::actions::resource_kinds::LABEL,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let label = state.services.labels().get(&name).await?;
    Ok(HttpResponse::Ok().json(LabelResponse::from_domain(&label)))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateLabelRequest {
    description: Option<String>,
}

/// Update a label
#[utoipa::path(
    patch,
    path = "/api/v1/inventory/labels/{name}",
    params(("name" = String, Path, description = "Label name")),
    request_body = UpdateLabelRequest,
    responses(
        (status = 200, description = "Label updated", body = LabelResponse),
        (status = 404, description = "Label not found")
    ),
    tag = "Inventory"
)]
#[patch("/inventory/labels/{name}")]
pub(crate) async fn update_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateLabelRequest>,
) -> Result<HttpResponse, AppError> {
    let name = LabelName::new(path.into_inner())?;
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::label::UPDATE_DESCRIPTION,
        authz::actions::resource_kinds::LABEL,
        name.as_str(),
    );
    if let Some(description) = &request.description {
        authz = authz.attr("new_description", AttrValue::String(description.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let command = UpdateLabel::new(request.description)?;
    let label = state.services.labels().update(&name, command).await?;
    Ok(HttpResponse::Ok().json(LabelResponse::from_domain(&label)))
}

/// Delete a label
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/labels/{name}",
    params(("name" = String, Path, description = "Label name")),
    responses(
        (status = 204, description = "Label deleted"),
        (status = 404, description = "Label not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/labels/{name}")]
pub(crate) async fn delete_label(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = LabelName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::label::DELETE,
            authz::actions::resource_kinds::LABEL,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    state.services.labels().delete(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_and_get_label() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        let create = test::TestRequest::post()
            .uri("/inventory/labels")
            .set_json(serde_json::json!({
                "name": "Production_Label",
                "description": "Production systems"
            }))
            .to_request();
        let response = test::call_service(&app, create).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let created: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(created["name"], "production_label");

        let get = test::TestRequest::get()
            .uri("/inventory/labels/production_label")
            .to_request();
        let response = test::call_service(&app, get).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["description"], "Production systems");
    }

    #[actix_web::test]
    async fn update_label_changes_description() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        // Create a label
        let create = test::TestRequest::post()
            .uri("/inventory/labels")
            .set_json(serde_json::json!({
                "name": "Environment_Label",
                "description": "Original description"
            }))
            .to_request();
        let response = test::call_service(&app, create).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let created: serde_json::Value = test::read_body_json(response).await;
        let original_updated_at = created["updated_at"].as_str().unwrap().to_string();

        // PATCH the label with a new description
        let patch = test::TestRequest::patch()
            .uri("/inventory/labels/environment_label")
            .set_json(serde_json::json!({
                "description": "Updated description"
            }))
            .to_request();
        let response = test::call_service(&app, patch).await;
        assert_eq!(response.status(), StatusCode::OK);

        // GET the label and verify the description changed
        let get = test::TestRequest::get()
            .uri("/inventory/labels/environment_label")
            .to_request();
        let response = test::call_service(&app, get).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["description"], "Updated description");
        let new_updated_at = body["updated_at"].as_str().unwrap().to_string();
        assert_ne!(original_updated_at, new_updated_at);
    }

    #[actix_web::test]
    async fn cursor_pagination_pages_through_labels() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        // Create 3 labels
        for name in ["alpha", "bravo", "charlie"] {
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri("/inventory/labels")
                    .set_json(serde_json::json!({
                        "name": name,
                        "description": format!("{name} label")
                    }))
                    .to_request(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CREATED);
        }

        // Page 1: limit=2
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/inventory/labels?limit=2")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 2);
        assert_eq!(body["total"], 3);
        let cursor = body["next_cursor"]
            .as_str()
            .expect("should have next cursor");

        // Page 2: use cursor
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/inventory/labels?limit=2&after={cursor}"))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["items"].as_array().unwrap().len(), 1);
        assert_eq!(body["total"], 3);
        assert!(body["next_cursor"].is_null(), "no more pages");
    }
}
