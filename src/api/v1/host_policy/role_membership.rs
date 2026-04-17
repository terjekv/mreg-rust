use actix_web::{HttpRequest, HttpResponse, delete, post, web};

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::types::HostPolicyName,
    errors::AppError,
    services::host_policy as hp_service,
};

use crate::api::v1::authz::request as authz_request;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(add_atom_to_role)
        .service(remove_atom_from_role)
        .service(add_host_to_role)
        .service(remove_host_from_role)
        .service(add_label_to_role)
        .service(remove_label_from_role);
}

// ---------------------------------------------------------------------------
// Role membership endpoints
// ---------------------------------------------------------------------------

/// Add an atom to a role
#[utoipa::path(
    post,
    path = "/api/v1/policy/host/roles/{name}/atoms/{atom}",
    params(
        ("name" = String, Path, description = "Role name"),
        ("atom" = String, Path, description = "Atom name"),
    ),
    responses(
        (status = 204, description = "Atom added to role"),
        (status = 404, description = "Role or atom not found"),
        (status = 409, description = "Atom already in role")
    ),
    tag = "Policy"
)]
#[post("/policy/host/roles/{name}/atoms/{atom}")]
pub(crate) async fn add_atom_to_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role_name, atom_name) = path.into_inner();
    let role_name = HostPolicyName::new(role_name)?;
    let atom_name = HostPolicyName::new(atom_name)?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::ATOM_ATTACH,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            role_name.as_str(),
        )
        .attr("atom", AttrValue::String(atom_name.as_str().to_string()))
        .build(),
    )
    .await?;
    hp_service::add_atom_to_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &role_name,
        &atom_name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Remove an atom from a role
#[utoipa::path(
    delete,
    path = "/api/v1/policy/host/roles/{name}/atoms/{atom}",
    params(
        ("name" = String, Path, description = "Role name"),
        ("atom" = String, Path, description = "Atom name"),
    ),
    responses(
        (status = 204, description = "Atom removed from role"),
        (status = 404, description = "Role or atom membership not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/host/roles/{name}/atoms/{atom}")]
pub(crate) async fn remove_atom_from_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role_name, atom_name) = path.into_inner();
    let role_name = HostPolicyName::new(role_name)?;
    let atom_name = HostPolicyName::new(atom_name)?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::ATOM_DETACH,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            role_name.as_str(),
        )
        .attr("atom", AttrValue::String(atom_name.as_str().to_string()))
        .build(),
    )
    .await?;
    hp_service::remove_atom_from_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &role_name,
        &atom_name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Add a host to a role
#[utoipa::path(
    post,
    path = "/api/v1/policy/host/roles/{name}/hosts/{host}",
    params(
        ("name" = String, Path, description = "Role name"),
        ("host" = String, Path, description = "Host name"),
    ),
    responses(
        (status = 204, description = "Host added to role"),
        (status = 404, description = "Role or host not found"),
        (status = 409, description = "Host already in role")
    ),
    tag = "Policy"
)]
#[post("/policy/host/roles/{name}/hosts/{host}")]
pub(crate) async fn add_host_to_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role_name, host_name) = path.into_inner();
    let role_name = HostPolicyName::new(role_name)?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::HOST_ATTACH,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            role_name.as_str(),
        )
        .attr("host", AttrValue::String(host_name.clone()))
        .build(),
    )
    .await?;
    hp_service::add_host_to_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &role_name,
        &host_name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Remove a host from a role
#[utoipa::path(
    delete,
    path = "/api/v1/policy/host/roles/{name}/hosts/{host}",
    params(
        ("name" = String, Path, description = "Role name"),
        ("host" = String, Path, description = "Host name"),
    ),
    responses(
        (status = 204, description = "Host removed from role"),
        (status = 404, description = "Role or host membership not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/host/roles/{name}/hosts/{host}")]
pub(crate) async fn remove_host_from_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role_name, host_name) = path.into_inner();
    let role_name = HostPolicyName::new(role_name)?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::HOST_DETACH,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            role_name.as_str(),
        )
        .attr("host", AttrValue::String(host_name.clone()))
        .build(),
    )
    .await?;
    hp_service::remove_host_from_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &role_name,
        &host_name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Add a label to a role
#[utoipa::path(
    post,
    path = "/api/v1/policy/host/roles/{name}/labels/{label}",
    params(
        ("name" = String, Path, description = "Role name"),
        ("label" = String, Path, description = "Label name"),
    ),
    responses(
        (status = 204, description = "Label added to role"),
        (status = 404, description = "Role or label not found"),
        (status = 409, description = "Label already in role")
    ),
    tag = "Policy"
)]
#[post("/policy/host/roles/{name}/labels/{label}")]
pub(crate) async fn add_label_to_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role_name, label_name) = path.into_inner();
    let role_name = HostPolicyName::new(role_name)?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::LABEL_ATTACH,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            role_name.as_str(),
        )
        .attr("label", AttrValue::String(label_name.clone()))
        .build(),
    )
    .await?;
    hp_service::add_label_to_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &role_name,
        &label_name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

/// Remove a label from a role
#[utoipa::path(
    delete,
    path = "/api/v1/policy/host/roles/{name}/labels/{label}",
    params(
        ("name" = String, Path, description = "Role name"),
        ("label" = String, Path, description = "Label name"),
    ),
    responses(
        (status = 204, description = "Label removed from role"),
        (status = 404, description = "Role or label membership not found")
    ),
    tag = "Policy"
)]
#[delete("/policy/host/roles/{name}/labels/{label}")]
pub(crate) async fn remove_label_from_role(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse, AppError> {
    let (role_name, label_name) = path.into_inner();
    let role_name = HostPolicyName::new(role_name)?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_policy::role::LABEL_DETACH,
            authz::actions::resource_kinds::HOST_POLICY_ROLE,
            role_name.as_str(),
        )
        .attr("label", AttrValue::String(label_name.clone()))
        .build(),
    )
    .await?;
    hp_service::remove_label_from_role(
        state.storage.host_policy(),
        state.storage.audit(),
        &state.events,
        &role_name,
        &label_name,
    )
    .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_role_and_add_atom() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        // Create an atom
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/atoms")
                .set_json(serde_json::json!({
                    "name": "monitoring",
                    "description": "Enable monitoring"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        // Create a role
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/roles")
                .set_json(serde_json::json!({
                    "name": "web-server",
                    "description": "Web server role"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let role: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(role["atoms"], serde_json::json!([]));

        // Add atom to role
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/policy/host/roles/web-server/atoms/monitoring")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Get role and verify atom is included
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/policy/host/roles/web-server")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let role: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(role["atoms"], serde_json::json!(["monitoring"]));

        // Remove atom from role
        let response = test::call_service(
            &app,
            test::TestRequest::delete()
                .uri("/policy/host/roles/web-server/atoms/monitoring")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify atom is removed
        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/policy/host/roles/web-server")
                .to_request(),
        )
        .await;
        let role: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(role["atoms"], serde_json::json!([]));
    }
}
