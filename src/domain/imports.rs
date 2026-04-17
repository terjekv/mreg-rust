use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::errors::AppError;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Label,
    Nameserver,
    Network,
    HostContact,
    HostGroup,
    BacnetId,
    PtrOverride,
    NetworkPolicy,
    NetworkPolicyAttribute,
    NetworkPolicyAttributeValue,
    Community,
    ForwardZone,
    ReverseZone,
    ForwardZoneDelegation,
    ReverseZoneDelegation,
    ExcludedRange,
    Host,
    HostAttachment,
    IpAddress,
    Record,
    AttachmentDhcpIdentifier,
    AttachmentPrefixReservation,
    AttachmentCommunityAssignment,
    HostCommunityAssignment,
    HostPolicyAtom,
    HostPolicyRole,
    HostPolicyRoleAtom,
    HostPolicyRoleHost,
    HostPolicyRoleLabel,
}

impl fmt::Display for ImportKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let json_value =
            serde_json::to_value(self).unwrap_or_else(|_| Value::String("unknown".to_string()));
        match json_value {
            Value::String(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportOperation {
    Create,
}

impl fmt::Display for ImportOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let json_value =
            serde_json::to_value(self).unwrap_or_else(|_| Value::String("unknown".to_string()));
        match json_value {
            Value::String(s) => write!(f, "{}", s),
            _ => write!(f, "{:?}", self),
        }
    }
}

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
    kind: ImportKind,
    operation: ImportOperation,
    #[serde(default)]
    attributes: Value,
}

impl ImportItem {
    pub fn new(
        reference: impl Into<String>,
        kind: ImportKind,
        operation: ImportOperation,
        attributes: Value,
    ) -> Result<Self, AppError> {
        let reference = reference.into().trim().to_string();

        if reference.is_empty() {
            return Err(AppError::validation("import item ref is required"));
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

    pub fn kind(&self) -> &ImportKind {
        &self.kind
    }

    pub fn operation(&self) -> &ImportOperation {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ImportItem;

    #[test]
    fn import_item_deserialization_rejects_unknown_kind() {
        let err = serde_json::from_value::<ImportItem>(json!({
            "ref": "item-1",
            "kind": "not_a_real_kind",
            "operation": "create",
            "attributes": {}
        }))
        .expect_err("unknown kind should fail deserialization");

        let message = err.to_string();
        assert!(message.contains("unknown variant"));
        assert!(message.contains("not_a_real_kind"));
    }

    #[test]
    fn import_item_deserialization_rejects_unknown_operation() {
        let err = serde_json::from_value::<ImportItem>(json!({
            "ref": "item-1",
            "kind": "host",
            "operation": "update",
            "attributes": {}
        }))
        .expect_err("unknown operation should fail deserialization");

        let message = err.to_string();
        assert!(message.contains("unknown variant"));
        assert!(message.contains("update"));
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
