use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::{
        record_validation::{
            preprocess_builtin_payload, validate_builtin_payload, validate_owner_name,
        },
        types::{DnsName, DnsTypeCode, RecordTypeName},
    },
    errors::AppError,
};

use super::{RawRdataValue, RecordTypeSchema, ValidatedRecordContent};

/// Complete definition of a DNS record type with validation and rendering schemas.
#[derive(Clone, Debug, Serialize)]
pub struct RecordTypeDefinition {
    id: Uuid,
    name: RecordTypeName,
    dns_type: Option<DnsTypeCode>,
    schema: RecordTypeSchema,
    built_in: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RecordTypeDefinition {
    pub fn restore(
        id: Uuid,
        name: RecordTypeName,
        dns_type: Option<DnsTypeCode>,
        schema: RecordTypeSchema,
        built_in: bool,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            dns_type,
            schema,
            built_in,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn name(&self) -> &RecordTypeName {
        &self.name
    }

    pub fn dns_type(&self) -> Option<DnsTypeCode> {
        self.dns_type
    }

    pub fn schema(&self) -> &RecordTypeSchema {
        &self.schema
    }

    pub fn built_in(&self) -> bool {
        self.built_in
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn validate_record_input(
        &self,
        owner_name: &DnsName,
        structured_data: Option<&Value>,
        raw_rdata: Option<&RawRdataValue>,
    ) -> Result<ValidatedRecordContent, AppError> {
        validate_owner_name(
            self.name(),
            owner_name.as_str(),
            self.schema().rfc_profile()?.as_ref(),
        )?;

        match (structured_data, raw_rdata) {
            (Some(_), Some(_)) => Err(AppError::validation(
                "record input must provide either structured data or raw_rdata, not both",
            )),
            (None, None) => Err(AppError::validation(
                "record input must provide structured data or raw_rdata",
            )),
            (Some(payload), None) => {
                let payload = preprocess_builtin_payload(self.name(), payload)?;
                let normalized = self.schema().validate_and_normalize(&payload)?;
                Ok(ValidatedRecordContent::Structured(
                    validate_builtin_payload(self.name(), &normalized)?,
                ))
            }
            (None, Some(raw_rdata)) => {
                if !self.schema().allows_raw_rdata() {
                    return Err(AppError::validation(format!(
                        "record type '{}' does not allow RFC 3597 raw RDATA input",
                        self.name().as_str()
                    )));
                }
                Ok(ValidatedRecordContent::RawRdata(raw_rdata.clone()))
            }
        }
    }
}

/// Command to register a new DNS record type definition.
#[derive(Clone, Debug)]
pub struct CreateRecordTypeDefinition {
    name: RecordTypeName,
    dns_type: Option<DnsTypeCode>,
    schema: RecordTypeSchema,
    built_in: bool,
}

impl CreateRecordTypeDefinition {
    pub fn new(
        name: RecordTypeName,
        dns_type: Option<DnsTypeCode>,
        schema: RecordTypeSchema,
        built_in: bool,
    ) -> Self {
        Self {
            name,
            dns_type,
            schema,
            built_in,
        }
    }

    pub fn name(&self) -> &RecordTypeName {
        &self.name
    }

    pub fn dns_type(&self) -> Option<DnsTypeCode> {
        self.dns_type
    }

    pub fn schema(&self) -> &RecordTypeSchema {
        &self.schema
    }

    pub fn built_in(&self) -> bool {
        self.built_in
    }
}
