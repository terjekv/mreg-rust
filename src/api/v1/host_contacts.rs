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
        filters::HostContactFilter,
        host_contact::HostContact,
        pagination::{PageRequest, PageResponse, SortDirection},
        types::{EmailAddressValue, Hostname},
    },
    errors::AppError,
};

use super::authz::{request as authz_request, string_set};

crate::page_response!(
    HostContactPageResponse,
    HostContactResponse,
    "Paginated list of host contacts."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_host_contacts)
        .service(create_host_contact)
        .service(get_host_contact)
        .service(delete_host_contact);
}

#[derive(Deserialize)]
pub struct HostContactQuery {
    after: Option<Uuid>,
    limit: Option<u64>,
    sort_by: Option<String>,
    sort_dir: Option<SortDirection>,
    search: Option<String>,
    #[serde(flatten)]
    filters: HashMap<String, String>,
}

impl HostContactQuery {
    fn into_parts(self) -> Result<(PageRequest, HostContactFilter), AppError> {
        let page = PageRequest {
            after: self.after,
            limit: self.limit,
            sort_by: self.sort_by,
            sort_dir: self.sort_dir,
        };
        let mut filter = HostContactFilter::from_query_params(self.filters)?;
        filter.search = self.search;
        Ok((page, filter))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateHostContactRequest {
    email: String,
    display_name: Option<String>,
    #[serde(default)]
    hosts: Vec<String>,
}

impl CreateHostContactRequest {
    fn into_command(self) -> Result<crate::domain::host_contact::CreateHostContact, AppError> {
        Ok(crate::domain::host_contact::CreateHostContact::new(
            EmailAddressValue::new(self.email)?,
            self.display_name,
            self.hosts
                .into_iter()
                .map(Hostname::new)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct HostContactResponse {
    id: Uuid,
    email: String,
    display_name: Option<String>,
    hosts: Vec<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostContactResponse {
    fn from_domain(contact: &HostContact) -> Self {
        Self {
            id: contact.id(),
            email: contact.email().as_str().to_string(),
            display_name: contact.display_name().map(str::to_string),
            hosts: contact
                .hosts()
                .iter()
                .map(|host| host.as_str().to_string())
                .collect(),
            created_at: contact.created_at(),
            updated_at: contact.updated_at(),
        }
    }
}

/// List host contacts
#[utoipa::path(
    get,
    path = "/api/v1/inventory/host-contacts",
    responses(
        (status = 200, description = "Paginated list of host contacts", body = HostContactPageResponse)
    ),
    tag = "Inventory"
)]
#[get("/inventory/host-contacts")]
pub(crate) async fn list_host_contacts(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<HostContactQuery>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_contact::LIST,
            authz::actions::resource_kinds::HOST_CONTACT,
            "*",
        )
        .build(),
    )
    .await?;
    let (page, filter) = query.into_inner().into_parts()?;
    let result = state.services.host_contacts().list(&page, &filter).await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        result,
        HostContactResponse::from_domain,
    )))
}

/// Create a host contact
#[utoipa::path(
    post,
    path = "/api/v1/inventory/host-contacts",
    request_body = CreateHostContactRequest,
    responses(
        (status = 201, description = "Host contact created", body = HostContactResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Contact already exists")
    ),
    tag = "Inventory"
)]
#[post("/inventory/host-contacts")]
pub(crate) async fn create_host_contact(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateHostContactRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::host_contact::CREATE,
        authz::actions::resource_kinds::HOST_CONTACT,
        request.email.clone(),
    )
    .attr("email", AttrValue::String(request.email.clone()))
    .attr("hosts", string_set(request.hosts.clone()));
    if let Some(display_name) = &request.display_name {
        authz = authz.attr("display_name", AttrValue::String(display_name.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let contact = state
        .services
        .host_contacts()
        .create(request.into_command()?)
        .await?;
    Ok(HttpResponse::Created().json(HostContactResponse::from_domain(&contact)))
}

/// Get a host contact by email
#[utoipa::path(
    get,
    path = "/api/v1/inventory/host-contacts/{email}",
    params(("email" = String, Path, description = "Contact email")),
    responses(
        (status = 200, description = "Host contact found", body = HostContactResponse),
        (status = 404, description = "Host contact not found")
    ),
    tag = "Inventory"
)]
#[get("/inventory/host-contacts/{email}")]
pub(crate) async fn get_host_contact(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let email = EmailAddressValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_contact::GET,
            authz::actions::resource_kinds::HOST_CONTACT,
            email.as_str(),
        )
        .build(),
    )
    .await?;
    let contact = state.services.host_contacts().get(&email).await?;
    Ok(HttpResponse::Ok().json(HostContactResponse::from_domain(&contact)))
}

/// Delete a host contact
#[utoipa::path(
    delete,
    path = "/api/v1/inventory/host-contacts/{email}",
    params(("email" = String, Path, description = "Contact email")),
    responses(
        (status = 204, description = "Host contact deleted"),
        (status = 404, description = "Host contact not found")
    ),
    tag = "Inventory"
)]
#[delete("/inventory/host-contacts/{email}")]
pub(crate) async fn delete_host_contact(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let email = EmailAddressValue::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_contact::DELETE,
            authz::actions::resource_kinds::HOST_CONTACT,
            email.as_str(),
        )
        .build(),
    )
    .await?;
    state.services.host_contacts().delete(&email).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_and_filter_host_contact() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(|cfg| crate::api::v1::configure(cfg, false)),
        )
        .await;

        let host_req = test::TestRequest::post()
            .uri("/inventory/hosts")
            .set_json(serde_json::json!({"name": "contact-test.example.org", "comment": "test"}))
            .to_request();
        let resp = test::call_service(&app, host_req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let create = test::TestRequest::post()
            .uri("/inventory/host-contacts")
            .set_json(serde_json::json!({
                "email": "admin@example.org",
                "display_name": "Admin User",
                "hosts": ["contact-test.example.org"]
            }))
            .to_request();
        let response = test::call_service(&app, create).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let list = test::TestRequest::get()
            .uri("/inventory/host-contacts?email__contains=admin")
            .to_request();
        let response = test::call_service(&app, list).await;
        assert_eq!(response.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(response).await;
        assert!(body["total"].as_u64().unwrap() >= 1);
    }
}
