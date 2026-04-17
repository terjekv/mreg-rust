use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{domain::types::LabelName, errors::AppError};

/// Named tag that can be attached to hosts for categorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    id: Uuid,
    name: LabelName,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Label {
    pub fn restore(
        id: Uuid,
        name: LabelName,
        description: String,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AppError> {
        if description.trim().is_empty() {
            return Err(AppError::validation("label description cannot be empty"));
        }
        Ok(Self {
            id,
            name,
            description,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &LabelName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new label with a required description.
#[derive(Clone, Debug)]
pub struct CreateLabel {
    name: LabelName,
    description: String,
}

impl CreateLabel {
    pub fn new(name: LabelName, description: impl Into<String>) -> Result<Self, AppError> {
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(AppError::validation("label description cannot be empty"));
        }
        Ok(Self { name, description })
    }

    pub fn name(&self) -> &LabelName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }
}

/// Partial update for a label's description.
#[derive(Clone, Debug)]
pub struct UpdateLabel {
    pub description: Option<String>,
}

impl UpdateLabel {
    pub fn new(description: Option<String>) -> Result<Self, AppError> {
        if let Some(ref desc) = description {
            let trimmed = desc.trim();
            if trimmed.is_empty() {
                return Err(AppError::validation("label description cannot be empty"));
            }
            return Ok(Self {
                description: Some(trimmed.to_string()),
            });
        }
        Ok(Self { description })
    }
}
