use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportBatch {
    items: Vec<ImportItem>,
}

impl ImportBatch {
    pub fn new(items: Vec<ImportItem>) -> Result<Self, AppError> {
        if items.is_empty() {
            return Err(AppError::validation("import batch cannot be empty"));
        }
        Ok(Self { items })
    }

    pub fn items(&self) -> &[ImportItem] {
        &self.items
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImportItem {
    #[serde(rename = "ref")]
    reference: String,
    kind: String,
    operation: String,
    #[serde(default)]
    attributes: Value,
}

impl ImportItem {
    pub fn new(
        reference: impl Into<String>,
        kind: impl Into<String>,
        operation: impl Into<String>,
        attributes: Value,
    ) -> Result<Self, AppError> {
        let reference = reference.into().trim().to_string();
        let kind = kind.into().trim().to_string();
        let operation = operation.into().trim().to_string();

        if reference.is_empty() || kind.is_empty() || operation.is_empty() {
            return Err(AppError::validation(
                "import item ref, kind, and operation are required",
            ));
        }

        Ok(Self {
            reference,
            kind,
            operation,
            attributes,
        })
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn attributes(&self) -> &Value {
        &self.attributes
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportBatchStatus {
    Queued,
    Validating,
    Ready,
    Committing,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportBatchSummary {
    id: Uuid,
    task_id: Option<Uuid>,
    status: ImportBatchStatus,
    requested_by: Option<String>,
    validation_report: Option<Value>,
    commit_summary: Option<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ImportBatchSummary {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        task_id: Option<Uuid>,
        status: ImportBatchStatus,
        requested_by: Option<String>,
        validation_report: Option<Value>,
        commit_summary: Option<Value>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            task_id,
            status,
            requested_by,
            validation_report,
            commit_summary,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn task_id(&self) -> Option<Uuid> {
        self.task_id
    }

    pub fn status(&self) -> &ImportBatchStatus {
        &self.status
    }

    pub fn requested_by(&self) -> Option<&str> {
        self.requested_by.as_deref()
    }

    pub fn validation_report(&self) -> Option<&Value> {
        self.validation_report.as_ref()
    }

    pub fn commit_summary(&self) -> Option<&Value> {
        self.commit_summary.as_ref()
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Clone, Debug)]
pub struct CreateImportBatch {
    batch: ImportBatch,
    requested_by: Option<String>,
}

impl CreateImportBatch {
    pub fn new(batch: ImportBatch, requested_by: Option<String>) -> Self {
        Self {
            batch,
            requested_by: requested_by.map(|value| value.trim().to_string()),
        }
    }

    pub fn batch(&self) -> &ImportBatch {
        &self.batch
    }

    pub fn requested_by(&self) -> Option<&str> {
        self.requested_by.as_deref()
    }
}
