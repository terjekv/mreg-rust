use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{PageRequest, PageResponse},
        types::{DnsName, Ttl, UpdateField},
    },
    errors::AppError,
};

use super::authz::request as authz_request;

crate::page_response!(
    NameServerPageResponse,
    NameServerResponse,
    "Paginated list of nameservers."
);

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_nameservers)
        .service(create_nameserver)
        .service(get_nameserver)
        .service(update_nameserver)
        .service(delete_nameserver);
}

#[derive(Deserialize, ToSchema)]
pub struct CreateNameServerRequest {
    name: String,
    ttl: Option<u32>,
}

impl CreateNameServerRequest {
    fn into_command(self) -> Result<CreateNameServer, AppError> {
        let ttl = self.ttl.map(Ttl::new).transpose()?;
        Ok(CreateNameServer::new(DnsName::new(self.name)?, ttl))
    }
}

#[derive(Serialize, ToSchema)]
pub struct NameServerResponse {
    id: Uuid,
    name: String,
    ttl: Option<u32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NameServerResponse {
    fn from_domain(nameserver: &NameServer) -> Self {
        Self {
            id: nameserver.id(),
            name: nameserver.name().as_str().to_string(),
            ttl: nameserver.ttl().map(|value| value.as_u32()),
            created_at: nameserver.created_at(),
            updated_at: nameserver.updated_at(),
        }
    }
}

/// List all nameservers
#[utoipa::path(
    get,
    path = "/api/v1/dns/nameservers",
    params(PageRequest),
    responses(
        (status = 200, description = "Paginated list of nameservers", body = NameServerPageResponse)
    ),
    tag = "DNS"
)]
#[get("/dns/nameservers")]
pub(crate) async fn list_nameservers(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PageRequest>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::nameserver::LIST,
            authz::actions::resource_kinds::NAMESERVER,
            "*",
        )
        .build(),
    )
    .await?;
    let page = state
        .services
        .nameservers()
        .list(&query.into_inner())
        .await?;
    Ok(HttpResponse::Ok().json(PageResponse::from_page(
        page,
        NameServerResponse::from_domain,
    )))
}

/// Create a new nameserver
#[utoipa::path(
    post,
    path = "/api/v1/dns/nameservers",
    request_body = CreateNameServerRequest,
    responses(
        (status = 201, description = "Nameserver created", body = NameServerResponse),
        (status = 400, description = "Validation error"),
        (status = 409, description = "Nameserver already exists")
    ),
    tag = "DNS"
)]
#[post("/dns/nameservers")]
pub(crate) async fn create_nameserver(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateNameServerRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::nameserver::CREATE,
        authz::actions::resource_kinds::NAMESERVER,
        request.name.clone(),
    );
    if let Some(ttl) = request.ttl {
        authz = authz.attr("ttl", AttrValue::Long(i64::from(ttl)));
    }
    require_permission(&state.authz, authz.build()).await?;
    let nameserver = state
        .services
        .nameservers()
        .create(request.into_command()?)
        .await?;

    Ok(HttpResponse::Created().json(NameServerResponse::from_domain(&nameserver)))
}

/// Get a nameserver by name
#[utoipa::path(
    get,
    path = "/api/v1/dns/nameservers/{name}",
    params(("name" = String, Path, description = "Nameserver FQDN")),
    responses(
        (status = 200, description = "Nameserver found", body = NameServerResponse),
        (status = 404, description = "Nameserver not found")
    ),
    tag = "DNS"
)]
#[get("/dns/nameservers/{name}")]
pub(crate) async fn get_nameserver(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = DnsName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::nameserver::GET,
            authz::actions::resource_kinds::NAMESERVER,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    let nameserver = state.services.nameservers().get(&name).await?;
    Ok(HttpResponse::Ok().json(NameServerResponse::from_domain(&nameserver)))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateNameServerRequest {
    #[serde(default)]
    #[schema(value_type = Option<u32>)]
    ttl: UpdateField<u32>,
}

/// Update a nameserver
#[utoipa::path(
    patch,
    path = "/api/v1/dns/nameservers/{name}",
    params(("name" = String, Path, description = "Nameserver FQDN")),
    request_body = UpdateNameServerRequest,
    responses(
        (status = 200, description = "Nameserver updated", body = NameServerResponse),
        (status = 404, description = "Nameserver not found")
    ),
    tag = "DNS"
)]
#[patch("/dns/nameservers/{name}")]
pub(crate) async fn update_nameserver(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateNameServerRequest>,
) -> Result<HttpResponse, AppError> {
    let name = DnsName::new(path.into_inner())?;
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::nameserver::UPDATE_TTL,
        authz::actions::resource_kinds::NAMESERVER,
        name.as_str(),
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
    require_permission(&state.authz, authz.build()).await?;
    let ttl = request.ttl.try_map(Ttl::new)?;
    let command = UpdateNameServer { ttl };
    let nameserver = state.services.nameservers().update(&name, command).await?;
    Ok(HttpResponse::Ok().json(NameServerResponse::from_domain(&nameserver)))
}

/// Delete a nameserver
#[utoipa::path(
    delete,
    path = "/api/v1/dns/nameservers/{name}",
    params(("name" = String, Path, description = "Nameserver FQDN")),
    responses(
        (status = 204, description = "Nameserver deleted"),
        (status = 404, description = "Nameserver not found")
    ),
    tag = "DNS"
)]
#[delete("/dns/nameservers/{name}")]
pub(crate) async fn delete_nameserver(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let name = DnsName::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::nameserver::DELETE,
            authz::actions::resource_kinds::NAMESERVER,
            name.as_str(),
        )
        .build(),
    )
    .await?;
    state.services.nameservers().delete(&name).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn create_and_get_nameserver() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let create = test::TestRequest::post()
            .uri("/dns/nameservers")
            .set_json(serde_json::json!({
                "name": "NS1.Example.Org.",
                "ttl": 3600
            }))
            .to_request();
        let response = test::call_service(&app, create).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let created: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(created["name"], "ns1.example.org");
        assert_eq!(created["ttl"], 3600);

        let get = test::TestRequest::get()
            .uri("/dns/nameservers/ns1.example.org")
            .to_request();
        let response = test::call_service(&app, get).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}
