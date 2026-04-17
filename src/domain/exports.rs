use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Clone, Debug, Serialize)]
pub struct ExportTemplate {
    id: Uuid,
    name: String,
    description: String,
    engine: String,
    scope: String,
    body: String,
    metadata: Value,
    builtin: bool,
}

impl ExportTemplate {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        name: impl Into<String>,
        description: impl Into<String>,
        engine: impl Into<String>,
        scope: impl Into<String>,
        body: impl Into<String>,
        metadata: Value,
        builtin: bool,
    ) -> Result<Self, AppError> {
        let name = name.into().trim().to_string();
        let engine = engine.into().trim().to_string();
        let scope = scope.into().trim().to_string();
        let body = body.into();

        if name.is_empty() || engine.is_empty() || scope.is_empty() || body.trim().is_empty() {
            return Err(AppError::validation(
                "export template name, engine, scope, and body are required",
            ));
        }

        Ok(Self {
            id,
            name,
            description: description.into().trim().to_string(),
            engine,
            scope,
            body,
            metadata,
            builtin,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn engine(&self) -> &str {
        &self.engine
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn metadata(&self) -> &Value {
        &self.metadata
    }

    pub fn builtin(&self) -> bool {
        self.builtin
    }
}

#[derive(Clone, Debug)]
pub struct CreateExportTemplate {
    name: String,
    description: String,
    engine: String,
    scope: String,
    body: String,
    metadata: Value,
}

impl CreateExportTemplate {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        engine: impl Into<String>,
        scope: impl Into<String>,
        body: impl Into<String>,
        metadata: Value,
    ) -> Result<Self, AppError> {
        let name = name.into().trim().to_string();
        let engine = engine.into().trim().to_string();
        let scope = scope.into().trim().to_string();
        let body = body.into();

        if name.is_empty() || engine.is_empty() || scope.is_empty() || body.trim().is_empty() {
            return Err(AppError::validation(
                "export template name, engine, scope, and body are required",
            ));
        }

        Ok(Self {
            name,
            description: description.into().trim().to_string(),
            engine,
            scope,
            body,
            metadata,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn engine(&self) -> &str {
        &self.engine
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn metadata(&self) -> &Value {
        &self.metadata
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExportRun {
    id: Uuid,
    task_id: Option<Uuid>,
    template_id: Option<Uuid>,
    requested_by: Option<String>,
    scope: String,
    parameters: Value,
    status: ExportRunStatus,
    rendered_output: Option<String>,
    artifact_metadata: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ExportRun {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        task_id: Option<Uuid>,
        template_id: Option<Uuid>,
        requested_by: Option<String>,
        scope: impl Into<String>,
        parameters: Value,
        status: ExportRunStatus,
        rendered_output: Option<String>,
        artifact_metadata: Option<Value>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        let scope = scope.into().trim().to_string();
        if scope.is_empty() {
            return Err(AppError::validation("export run scope cannot be empty"));
        }

        Ok(Self {
            id,
            task_id,
            template_id,
            requested_by,
            scope,
            parameters,
            status,
            rendered_output,
            artifact_metadata,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn task_id(&self) -> Option<Uuid> {
        self.task_id
    }

    pub fn template_id(&self) -> Option<Uuid> {
        self.template_id
    }

    pub fn requested_by(&self) -> Option<&str> {
        self.requested_by.as_deref()
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn parameters(&self) -> &Value {
        &self.parameters
    }

    pub fn status(&self) -> &ExportRunStatus {
        &self.status
    }

    pub fn rendered_output(&self) -> Option<&str> {
        self.rendered_output.as_deref()
    }

    pub fn artifact_metadata(&self) -> Option<&Value> {
        self.artifact_metadata.as_ref()
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateExportRun {
    template_name: String,
    requested_by: Option<String>,
    scope: String,
    parameters: Value,
}

impl CreateExportRun {
    pub fn new(
        template_name: impl Into<String>,
        requested_by: Option<String>,
        scope: impl Into<String>,
        parameters: Value,
    ) -> Result<Self, AppError> {
        let template_name = template_name.into().trim().to_string();
        let scope = scope.into().trim().to_string();
        if template_name.is_empty() || scope.is_empty() {
            return Err(AppError::validation(
                "export run template name and scope are required",
            ));
        }

        Ok(Self {
            template_name,
            requested_by: requested_by.map(|value| value.trim().to_string()),
            scope,
            parameters,
        })
    }

    pub fn template_name(&self) -> &str {
        &self.template_name
    }

    pub fn requested_by(&self) -> Option<&str> {
        self.requested_by.as_deref()
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn parameters(&self) -> &Value {
        &self.parameters
    }
}
