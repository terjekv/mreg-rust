use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        host_policy::{CreateHostPolicyAtom, HostPolicyAtom, UpdateHostPolicyAtom},
        pagination::{PageRequest, PageResponse},
        types::HostPolicyName,
    },
    errors::AppError,
    services::host_policy as hp_service,
};

use crate::api::v1::authz::request as authz_request;

crate::page_response!(
    AtomPageResponse,
    AtomResponse,
    "Paginated list of host-policy atoms."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_atoms)
        .service(create_atom)
        .service(get_atom)
        .service(update_atom)
        .service(delete_atom);
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Request body for creating a host-policy atom.
#[derive(Deserialize, ToSchema)]
pub struct CreateAtomRequest {
    name: String,
    description: String,
}

impl CreateAtomRequest {
    fn into_command(self) -> Result<CreateHostPolicyAtom, AppError> {
        Ok(CreateHostPolicyAtom::new(
            HostPolicyName::new(self.name)?,
            self.description,
        ))
    }
}

/// Request body for updating a host-policy atom.
#[derive(Deserialize, ToSchema)]
pub struct UpdateAtomRequest {
    description: Option<String>,
}

/// Response body for a host-policy atom.
#[derive(Serialize, ToSchema)]
pub struct AtomResponse {
    id: Uuid,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AtomResponse {
    pub(crate) fn from_domain(atom: &HostPolicyAtom) -> Self {
        Self {
            id: atom.id(),
            name: atom.name().as_str().to_string(),
            description: atom.description().to_string(),
            created_at: atom.created_at(),
            updated_at: atom.updated_at(),
        }
    }
}

// ---------------------------------------------------------------------------
// Atom endpoints
// ---------------------------------------------------------------------------

/// List all host-policy atoms
#[utoipa::path(
    get,
    path = "/api/v1/policy/host/atoms",
    params(PageRequest),
    responses(
        (status = 200, description = "Paginated list of atoms", body = AtomPageResponse)
    ),
    tag = "Policy"
)]
#[get("/policy/host/atoms")]
pub(crate) async fn list_atoms(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::atom::LIST,
            authz::actions::resource_kinds::HOST_POLICY_ATOM,
            "*",
        )
        .build(),
    )
    .await?;
    let page = hp_service::list_atoms(state.storage.host_policy(), &query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(page, AtomResponse::from_domain)))
}

/// Create a new host-policy atom
#[utoipa::path(
    post,
    path = "/api/v1/policy/host/atoms",
    request_body = CreateAtomRequest,
    responses(
        (status = 201, description = "Atom created", body = AtomResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Atom already exists")
    ),
    tag = "Policy"
)]
#[post("/policy/host/atoms")]
pub(crate) async fn create_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateAtomRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::atom::CREATE,
            authz::actions::resource_kinds::HOST_POLICY_ATOM,
            request.name.clone(),
        )
        .attr(
            "description",
            AttrValue::String(request.description.clone()),
        )
        .build(),
    )
    .await?;
    let atom = hp_service::create_atom(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        request.into_command()?,
    )
    .await?;
    Ok(HttpResponse::Created().json(AtomResponse::from_domain(&atom)))
}

/// Get a host-policy atom by name
#[utoipa::path(
    get,
    path = "/api/v1/policy/host/atoms/{name}",
    params(("name" = String, Path, description = "Atom name")),
    responses(
        (status = 200, description = "Atom found", body = AtomResponse),
        (status = 404, description = "Atom not found")
    ),
    tag = "Policy"
)]
#[get("/policy/host/atoms/{name}")]
pub(crate) async fn get_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::atom::GET,
            authz::actions::resource_kinds::HOST_POLICY_ATOM,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let atom = hp_service::get_atom(state.storage.host_policy(), &name).await?;
    Ok(HttpResponse::Ok().json(AtomResponse::from_domain(&atom)))
}

/// Update a host-policy atom's description
#[utoipa::path(
    patch,
    path = "/api/v1/policy/host/atoms/{name}",
    params(("name" = String, Path, description = "Atom name")),
    request_body = UpdateAtomRequest,
    responses(
        (status = 200, description = "Atom updated", body = AtomResponse),
        (status = 404, description = "Atom not found")
    ),
    tag = "Policy"
)]
#[patch("/policy/host/atoms/{name}")]
pub(crate) async fn update_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateAtomRequest>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(path.into_inner())?;
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::host_policy::atom::UPDATE_DESCRIPTION,
        authz::actions::resource_kinds::HOST_POLICY_ATOM,
        name.as_str(),
    );
    if let Some(description) = &request.description {
        authz = authz.attr("new_description", AttrValue::String(description.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let command = UpdateHostPolicyAtom {
        description: request.description,
    };
    let atom = hp_service::update_atom(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &name,
        command,
    )
    .await?;
    Ok(HttpResponse::Ok().json(AtomResponse::from_domain(&atom)))
}

/// Delete a host-policy atom
#[utoipa::path(
    delete,
    path = "/api/v1/policy/host/atoms/{name}",
    params(("name" = String, Path, description = "Atom name")),
    responses(
        (status = 204, description = "Atom deleted"),
        (status = 404, description = "Atom not found"),
        (status = 409, description = "Atom is in use by a role")
    ),
    tag = "Policy"
)]
#[delete("/policy/host/atoms/{name}")]
pub(crate) async fn delete_atom(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = HostPolicyName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::atom::DELETE,
            authz::actions::resource_kinds::HOST_POLICY_ATOM,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    hp_service::delete_atom(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_and_get_atom() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let create = test::TestRequest::post()
            .uri("/policy/host/atoms")
            .set_json(serde_json::json!({
                "name": "SSH_Access",
                "description": "Allow SSH access"
            }))
            .to_request();
        let response = test::call_service(&app, create).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let created: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(created["name"], "ssh_access");

        let get = test::TestRequest::get()
            .uri("/policy/host/atoms/ssh_access")
            .to_request();
        let response = test::call_service(&app, get).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["description"], "Allow SSH access");
    }

    #[actix_web::test]
    async fn delete_atom_in_use_returns_conflict() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        // Create atom and role
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/atoms")
                .set_json(serde_json::json!({
                    "name": "logging",
                    "description": "Enable logging"
                }))
                .to_request(),
        )
        .await;

        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/roles")
                .set_json(serde_json::json!({
                    "name": "app-server",
                    "description": "App server"
                }))
                .to_request(),
        )
        .await;

        // Add atom to role
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/roles/app-server/atoms/logging")
                .to_request(),
        )
        .await;

        // Try to delete atom -- should fail with 409
        let response = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri("/policy/host/atoms/logging")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[actix_web::test]
    async fn update_atom_description() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        // Create atom
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/atoms")
                .set_json(serde_json::json!({
                    "name": "ntp",
                    "description": "Original"
                }))
                .to_request(),
        )
        .await;

        // Update description
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri("/policy/host/atoms/ntp")
                .set_json(serde_json::json!({
                    "description": "Updated NTP policy"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(body["description"], "Updated NTP policy");
    }
}
