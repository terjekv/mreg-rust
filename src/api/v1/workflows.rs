use actix_web::{HttpRequest, HttpResponse, post, web};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        exports::{CreateExportRun, CreateExportTemplate},
        imports::{CreateImportBatch, ImportBatch, ImportItem},
        tasks::TaskEnvelope,
    },
    errors::AppError,
    services::{exports as export_service, imports as import_service, tasks as task_service},
};

use super::authz::request as authz_request;

/// Task execution result.
#[derive(Serialize, ToSchema)]
pub struct TaskRunResponse {
    /// The completed or no-op task.
    #[schema(value_type = Option<Object>)]
    pub task: Value,
    /// Workflow-specific result, if any.
    #[schema(value_type = Option<Object>)]
    pub workflow_result: Value,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(create_import)
        .service(create_export_template)
        .service(create_export_run)
        .service(run_next_task);
}

#[derive(Deserialize, ToSchema)]
pub struct CreateImportRequest {
    requested_by: Option<String>,
    items: Vec<CreateImportItemRequest>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateImportItemRequest {
    #[serde(rename = "ref")]
    reference: String,
    kind: String,
    operation: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    attributes: Value,
}

impl CreateImportRequest {
    fn into_command(self) -> Result<CreateImportBatch, AppError> {
        let items = self
            .items
            .into_iter()
            .map(|item| ImportItem::new(item.reference, item.kind, item.operation, item.attributes))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CreateImportBatch::new(
            ImportBatch::new(items)?,
            self.requested_by,
        ))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateExportTemplateRequest {
    name: String,
    #[serde(default)]
    description: String,
    engine: String,
    scope: String,
    body: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    metadata: Value,
}

impl CreateExportTemplateRequest {
    fn into_command(self) -> Result<CreateExportTemplate, AppError> {
        CreateExportTemplate::new(
            self.name,
            self.description,
            self.engine,
            self.scope,
            self.body,
            self.metadata,
        )
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateExportRunRequest {
    template_name: String,
    requested_by: Option<String>,
    scope: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    parameters: Value,
}

impl CreateExportRunRequest {
    fn into_command(self) -> Result<CreateExportRun, AppError> {
        CreateExportRun::new(
            self.template_name,
            self.requested_by,
            self.scope,
            self.parameters,
        )
    }
}

/// Create an import batch
#[utoipa::path(
    post,
    path = "/api/v1/workflows/imports",
    request_body = CreateImportRequest,
    responses(
        (status = 201, description = "Import batch created", body = Value),
        (status = 400, description = "Validation error")
    ),
    tag = "Workflows"
)]
#[post("/workflows/imports")]
pub(crate) async fn create_import(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateImportRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::import_batch::CREATE,
            authz::actions::resource_kinds::IMPORT_BATCH,
            request
                .requested_by
                .clone()
                .unwrap_or_else(|| "anonymous".to_string()),
        )
        .attr(
            "item_count",
            AttrValue::Long(i64::try_from(request.items.len()).unwrap_or(i64::MAX)),
        )
        .build(),
    )
    .await?;
    let summary = import_service::create(state.storage.imports(), request.into_command()?).await?;
    Ok(HttpResponse::Created().json(summary))
}

/// Create an export template
#[utoipa::path(
    post,
    path = "/api/v1/workflows/export-templates",
    request_body = CreateExportTemplateRequest,
    responses(
        (status = 201, description = "Export template created", body = Value),
        (status = 400, description = "Validation error")
    ),
    tag = "Workflows"
)]
#[post("/workflows/export-templates")]
pub(crate) async fn create_export_template(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateExportTemplateRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::export_template::CREATE,
            authz::actions::resource_kinds::EXPORT_TEMPLATE,
            request.name.clone(),
        )
        .attr("engine", AttrValue::String(request.engine.clone()))
        .attr("scope", AttrValue::String(request.scope.clone()))
        .build(),
    )
    .await?;
    let template =
        export_service::create_template(state.storage.exports(), request.into_command()?).await?;
    Ok(HttpResponse::Created().json(template))
}

/// Create an export run
#[utoipa::path(
    post,
    path = "/api/v1/workflows/export-runs",
    request_body = CreateExportRunRequest,
    responses(
        (status = 201, description = "Export run created", body = Value),
        (status = 400, description = "Validation error")
    ),
    tag = "Workflows"
)]
#[post("/workflows/export-runs")]
pub(crate) async fn create_export_run(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateExportRunRequest>,
) -> Result<HttpResponse, AppError> {
    let request = payload.into_inner();
    let mut authz = authz_request(
        &req,
        authz::actions::export_run::CREATE,
        authz::actions::resource_kinds::EXPORT_RUN,
        request.template_name.clone(),
    )
    .attr(
        "template_name",
        AttrValue::String(request.template_name.clone()),
    )
    .attr("scope", AttrValue::String(request.scope.clone()));
    if let Some(requested_by) = &request.requested_by {
        authz = authz.attr("requested_by", AttrValue::String(requested_by.clone()));
    }
    require_permission(&state.authz, authz.build()).await?;
    let run = export_service::create_run(state.storage.exports(), request.into_command()?).await?;
    Ok(HttpResponse::Created().json(run))
}

/// Run the next pending task
#[utoipa::path(
    post,
    path = "/api/v1/workflows/tasks/run-next",
    responses(
        (status = 200, description = "Task result or no task available", body = TaskRunResponse)
    ),
    tag = "Workflows"
)]
#[post("/workflows/tasks/run-next")]
pub(crate) async fn run_next_task(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Result<HttpResponse, AppError> {
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::worker::TASK_CLAIM_NEXT,
            authz::actions::resource_kinds::TASK,
            "next",
        )
        .build(),
    )
    .await?;
    let Some(task) = task_service::claim_next(state.storage.tasks()).await? else {
        return Ok(HttpResponse::Ok().json(json!({ "task": Value::Null })));
    };

    let workflow_result = match task.kind() {
        "import_batch" => execute_import_task(&req, &state, &task).await?,
        "export_run" => execute_export_task(&req, &state, &task).await?,
        _ => {
            let completed =
                task_service::complete(state.storage.tasks(), task.id(), json!({"status":"noop"}))
                    .await?;
            return Ok(HttpResponse::Ok().json(json!({
                "task": completed,
                "workflow_result": Value::Null,
            })));
        }
    };

    let task =
        task_service::complete(state.storage.tasks(), task.id(), workflow_result.clone()).await?;
    Ok(HttpResponse::Ok().json(json!({
        "task": task,
        "workflow_result": workflow_result,
    })))
}

/// Execute an import_batch task: check permissions, parse the import ID from
/// the task payload, and run the import.  On failure the task is marked as
/// failed before the error is propagated.
async fn execute_import_task(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    task: &TaskEnvelope,
) -> Result<Value, AppError> {
    if let Err(error) = require_permission(
        &state.authz,
        authz_request(
            req,
            authz::actions::worker::TASK_EXECUTE_IMPORT_BATCH,
            authz::actions::resource_kinds::TASK,
            task.id().to_string(),
        )
        .attr("kind", AttrValue::String(task.kind().to_string()))
        .build(),
    )
    .await
    {
        let _ = task_service::fail(state.storage.tasks(), task.id(), error.to_string()).await;
        return Err(error);
    }
    let import_id = parse_task_uuid(task.payload(), "import_id")?;
    match import_service::run(state.storage.imports(), import_id).await {
        Ok(summary) => serde_json::to_value(summary).map_err(AppError::internal),
        Err(error) => {
            let _ = task_service::fail(state.storage.tasks(), task.id(), error.to_string()).await;
            Err(error)
        }
    }
}

/// Execute an export_run task: check permissions, parse the run ID from the
/// task payload, and run the export.  On failure the task is marked as failed
/// before the error is propagated.
async fn execute_export_task(
    req: &HttpRequest,
    state: &web::Data<AppState>,
    task: &TaskEnvelope,
) -> Result<Value, AppError> {
    if let Err(error) = require_permission(
        &state.authz,
        authz_request(
            req,
            authz::actions::worker::TASK_EXECUTE_EXPORT_RUN,
            authz::actions::resource_kinds::TASK,
            task.id().to_string(),
        )
        .attr("kind", AttrValue::String(task.kind().to_string()))
        .build(),
    )
    .await
    {
        let _ = task_service::fail(state.storage.tasks(), task.id(), error.to_string()).await;
        return Err(error);
    }
    let run_id = parse_task_uuid(task.payload(), "run_id")?;
    match export_service::run_export(state.storage.exports(), run_id).await {
        Ok(run) => serde_json::to_value(run).map_err(AppError::internal),
        Err(error) => {
            let _ = task_service::fail(state.storage.tasks(), task.id(), error.to_string()).await;
            Err(error)
        }
    }
}

fn parse_task_uuid(payload: &Value, key: &str) -> Result<Uuid, AppError> {
    let raw = payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal(format!("task payload is missing '{}'", key)))?;
    raw.parse::<Uuid>().map_err(|error| {
        AppError::internal(format!("invalid task payload uuid for '{}': {error}", key))
    })
}

#[cfg(test)]
mod tests {
    use actix_web::{App, http::StatusCode, test, web};

    use crate::api::v1::tests::test_state;

    #[actix_web::test]
    async fn import_batch_is_atomic_when_one_item_is_invalid() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let create_import = test::TestRequest::post()
            .uri("/workflows/imports")
            .set_json(serde_json::json!({
                "requested_by": "tester",
                "items": [
                    {
                        "ref": "network-1",
                        "kind": "network",
                        "operation": "create",
                        "attributes": {
                            "cidr": "10.10.0.0/24",
                            "description": "Import network"
                        }
                    },
                    {
                        "ref": "bad-host",
                        "kind": "host",
                        "operation": "create",
                        "attributes": {
                            "name": "bad.example.org",
                            "zone": "missing.example.org"
                        }
                    }
                ]
            }))
            .to_request();
        let response = test::call_service(&app, create_import).await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/workflows/tasks/run-next")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let networks = test::call_service(
            &app,
            test::TestRequest::get()
                .uri("/inventory/networks")
                .to_request(),
        )
        .await;
        let body: serde_json::Value = test::read_body_json(networks).await;
        assert_eq!(body["items"], serde_json::json!([]));
    }

    #[actix_web::test]
    async fn export_run_renders_memory_backed_context() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(test_state()))
                .configure(crate::api::v1::configure),
        )
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/inventory/networks")
                .set_json(serde_json::json!({
                    "cidr": "10.0.0.0/24",
                    "description": "Prod",
                    "reserved": 3
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/workflows/export-templates")
                .set_json(serde_json::json!({
                    "name": "network-report",
                    "description": "Simple report",
                    "engine": "minijinja",
                    "scope": "inventory",
                    "body": "{% for network in networks %}{{ network.cidr }} {{ network.description }}{% endfor %}"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/workflows/export-runs")
                .set_json(serde_json::json!({
                    "template_name": "network-report",
                    "scope": "inventory"
                }))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/workflows/tasks/run-next")
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = test::read_body_json(response).await;
        assert_eq!(
            body["workflow_result"]["rendered_output"],
            "10.0.0.0/24 Prod"
        );
    }
}
