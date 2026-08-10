use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::types::{DnsName, RecordTypeName, Ttl, UpdateField},
    errors::AppError,
};

use super::{DnsClass, RawRdataValue, RecordOwnerKind};

/// DNS Resource Record Set (RFC 2181) -- groups records of the same type at the same owner.
#[derive(Clone, Debug, Serialize)]
pub struct RecordRrset {
    id: Uuid,
    type_id: Uuid,
    type_name: RecordTypeName,
    dns_class: DnsClass,
    owner_name: DnsName,
    anchor_kind: Option<RecordOwnerKind>,
    anchor_id: Option<Uuid>,
    anchor_name: Option<String>,
    zone_id: Option<Uuid>,
    ttl: Option<Ttl>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RecordRrset {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        type_id: Uuid,
        type_name: RecordTypeName,
        dns_class: DnsClass,
        owner_name: DnsName,
        anchor_kind: Option<RecordOwnerKind>,
        anchor_id: Option<Uuid>,
        anchor_name: Option<String>,
        zone_id: Option<Uuid>,
        ttl: Option<Ttl>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            type_id,
            type_name,
            dns_class,
            owner_name,
            anchor_kind,
            anchor_id,
            anchor_name,
            zone_id,
            ttl,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn type_id(&self) -> Uuid {
        self.type_id
    }

    pub fn type_name(&self) -> &RecordTypeName {
        &self.type_name
    }

    pub fn dns_class(&self) -> &DnsClass {
        &self.dns_class
    }

    pub fn owner_name(&self) -> &DnsName {
        &self.owner_name
    }

    pub fn anchor_kind(&self) -> Option<&RecordOwnerKind> {
        self.anchor_kind.as_ref()
    }

    pub fn anchor_id(&self) -> Option<Uuid> {
        self.anchor_id
    }

    pub fn anchor_name(&self) -> Option<&str> {
        self.anchor_name.as_deref()
    }

    pub fn zone_id(&self) -> Option<Uuid> {
        self.zone_id
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Individual DNS resource record within an RRSet.
#[derive(Clone, Debug, Serialize)]
pub struct RecordInstance {
    id: Uuid,
    rrset_id: Uuid,
    type_id: Uuid,
    type_name: RecordTypeName,
    owner_kind: Option<RecordOwnerKind>,
    owner_id: Option<Uuid>,
    owner_name: DnsName,
    zone_id: Option<Uuid>,
    ttl: Option<Ttl>,
    data: Value,
    raw_rdata: Option<RawRdataValue>,
    rendered: Option<String>,
    legacy_compatibility: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RecordInstance {
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        rrset_id: Uuid,
        type_id: Uuid,
        type_name: RecordTypeName,
        owner_kind: Option<RecordOwnerKind>,
        owner_id: Option<Uuid>,
        owner_name: DnsName,
        zone_id: Option<Uuid>,
        ttl: Option<Ttl>,
        data: Value,
        raw_rdata: Option<RawRdataValue>,
        rendered: Option<String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        let legacy_compatibility = match type_name.as_str() {
            "NAPTR" => crate::domain::record_validation::validate_naptr_payload(&data).is_err(),
            "SSHFP" => crate::domain::record_validation::validate_sshfp_payload(&data).is_err(),
            _ => false,
        };
        Self {
            id,
            rrset_id,
            type_id,
            type_name,
            owner_kind,
            owner_id,
            owner_name,
            zone_id,
            ttl,
            data,
            raw_rdata,
            rendered,
            legacy_compatibility,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn rrset_id(&self) -> Uuid {
        self.rrset_id
    }

    pub fn type_id(&self) -> Uuid {
        self.type_id
    }

    pub fn type_name(&self) -> &RecordTypeName {
        &self.type_name
    }

    pub fn owner_kind(&self) -> Option<&RecordOwnerKind> {
        self.owner_kind.as_ref()
    }

    pub fn owner_id(&self) -> Option<Uuid> {
        self.owner_id
    }

    pub fn owner_name(&self) -> &str {
        self.owner_name.as_str()
    }

    pub fn zone_id(&self) -> Option<Uuid> {
        self.zone_id
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn data(&self) -> &Value {
        &self.data
    }

    pub fn raw_rdata(&self) -> Option<&RawRdataValue> {
        self.raw_rdata.as_ref()
    }

    pub fn rendered(&self) -> Option<&str> {
        self.rendered.as_deref()
    }

    /// True when the stored structured payload is a faithfully retained V1
    /// value that does not satisfy the canonical V2 DNS invariant.
    pub fn legacy_compatibility(&self) -> bool {
        self.legacy_compatibility
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Command to create a new DNS resource record.
#[derive(Clone, Debug)]
pub struct CreateRecordInstance {
    type_name: RecordTypeName,
    owner_kind: Option<RecordOwnerKind>,
    owner_name: DnsName,
    anchor_name: Option<String>,
    ttl: Option<Ttl>,
    data: Option<Value>,
    raw_rdata: Option<RawRdataValue>,
    legacy_compatibility: bool,
}

impl CreateRecordInstance {
    pub fn new(
        type_name: RecordTypeName,
        owner_kind: RecordOwnerKind,
        owner_name: impl Into<String>,
        ttl: Option<Ttl>,
        data: Value,
    ) -> Result<Self, AppError> {
        let owner_name = owner_name.into();
        Ok(Self {
            type_name,
            owner_kind: Some(owner_kind),
            anchor_name: Some(owner_name.trim().to_string()),
            owner_name: DnsName::new(&owner_name)?,
            ttl,
            data: Some(data),
            raw_rdata: None,
            legacy_compatibility: false,
        })
    }

    pub fn new_unanchored(
        type_name: RecordTypeName,
        owner_name: impl Into<String>,
        ttl: Option<Ttl>,
        data: Value,
    ) -> Result<Self, AppError> {
        let owner_name = owner_name.into();
        Ok(Self {
            type_name,
            owner_kind: None,
            anchor_name: None,
            owner_name: DnsName::new(&owner_name)?,
            ttl,
            data: Some(data),
            raw_rdata: None,
            legacy_compatibility: false,
        })
    }

    pub fn new_anchored(
        type_name: RecordTypeName,
        owner_kind: RecordOwnerKind,
        owner_name: impl Into<String>,
        anchor_name: impl Into<String>,
        ttl: Option<Ttl>,
        data: Value,
    ) -> Result<Self, AppError> {
        let owner_name = owner_name.into();
        Ok(Self {
            type_name,
            owner_kind: Some(owner_kind),
            anchor_name: Some(anchor_name.into().trim().to_string()),
            owner_name: DnsName::new(&owner_name)?,
            ttl,
            data: Some(data),
            raw_rdata: None,
            legacy_compatibility: false,
        })
    }

    pub fn new_raw(
        type_name: RecordTypeName,
        owner_kind: Option<RecordOwnerKind>,
        owner_name: impl Into<String>,
        anchor_name: Option<String>,
        ttl: Option<Ttl>,
        raw_rdata: RawRdataValue,
    ) -> Result<Self, AppError> {
        let owner_name = owner_name.into();
        Ok(Self {
            type_name,
            owner_kind,
            anchor_name,
            owner_name: DnsName::new(&owner_name)?,
            ttl,
            data: None,
            raw_rdata: Some(raw_rdata),
            legacy_compatibility: false,
        })
    }

    pub fn with_reference(
        type_name: RecordTypeName,
        owner_kind: Option<RecordOwnerKind>,
        owner_name: impl Into<String>,
        anchor_name: Option<String>,
        ttl: Option<Ttl>,
        data: Option<Value>,
        raw_rdata: Option<RawRdataValue>,
    ) -> Result<Self, AppError> {
        let owner_name = owner_name.into();
        Ok(Self {
            type_name,
            owner_kind,
            anchor_name,
            owner_name: DnsName::new(&owner_name)?,
            ttl,
            data,
            raw_rdata,
            legacy_compatibility: false,
        })
    }

    pub fn type_name(&self) -> &RecordTypeName {
        &self.type_name
    }

    pub fn owner_kind(&self) -> Option<&RecordOwnerKind> {
        self.owner_kind.as_ref()
    }

    pub fn owner_name(&self) -> &DnsName {
        &self.owner_name
    }

    pub fn anchor_name(&self) -> Option<&str> {
        self.anchor_name.as_deref()
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn data(&self) -> Option<&Value> {
        self.data.as_ref()
    }

    pub fn raw_rdata(&self) -> Option<&RawRdataValue> {
        self.raw_rdata.as_ref()
    }

    pub(crate) fn with_legacy_compatibility(mut self) -> Self {
        self.legacy_compatibility = true;
        self
    }

    pub fn legacy_compatibility(&self) -> bool {
        self.legacy_compatibility
    }
}

/// Command to update an existing DNS resource record's TTL or data.
#[derive(Clone, Debug)]
pub struct UpdateRecord {
    ttl: UpdateField<Ttl>,
    data: Option<Value>, // None=don't change, Some(v)=new structured data
    raw_rdata: Option<RawRdataValue>, // None=don't change, Some(r)=new raw rdata
}

impl UpdateRecord {
    pub fn new(
        ttl: UpdateField<Ttl>,
        data: Option<Value>,
        raw_rdata: Option<RawRdataValue>,
    ) -> Result<Self, AppError> {
        if data.is_some() && raw_rdata.is_some() {
            return Err(AppError::validation(
                "update must provide either structured data or raw_rdata, not both",
            ));
        }
        Ok(Self {
            ttl,
            data,
            raw_rdata,
        })
    }

    pub fn ttl(&self) -> UpdateField<Ttl> {
        self.ttl
    }

    pub fn data(&self) -> Option<&Value> {
        self.data.as_ref()
    }

    pub fn raw_rdata(&self) -> Option<&RawRdataValue> {
        self.raw_rdata.as_ref()
    }
}

/// Lightweight summary of an existing record used for relationship validation.
#[derive(Clone, Debug)]
pub struct ExistingRecordSummary {
    type_name: RecordTypeName,
    ttl: Option<Ttl>,
    data: Value,
    raw_rdata: Option<RawRdataValue>,
}

impl ExistingRecordSummary {
    pub fn new(
        type_name: RecordTypeName,
        ttl: Option<Ttl>,
        data: Value,
        raw_rdata: Option<RawRdataValue>,
    ) -> Self {
        Self {
            type_name,
            ttl,
            data,
            raw_rdata,
        }
    }

    pub fn type_name(&self) -> &RecordTypeName {
        &self.type_name
    }

    pub fn ttl(&self) -> Option<Ttl> {
        self.ttl
    }

    pub fn data(&self) -> &Value {
        &self.data
    }

    pub fn raw_rdata(&self) -> Option<&RawRdataValue> {
        self.raw_rdata.as_ref()
    }
}
