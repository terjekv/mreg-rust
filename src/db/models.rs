use chrono::{DateTime, Utc};
use diesel::{
    Queryable, QueryableByName, Selectable,
    sql_types::{BigInt, Bool, Bytea, Integer, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    audit::HistoryEvent,
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            DhcpIdentifierFamily, DhcpIdentifierKind, HostAttachment,
        },
        exports::{ExportRun, ExportRunStatus, ExportTemplate},
        host::{Host, IpAddressAssignment},
        imports::{ImportBatch, ImportBatchStatus, ImportBatchSummary},
        label::Label,
        nameserver::NameServer,
        network::{ExcludedRange, Network},
        resource_records::{
            DnsClass, RawRdataValue, RecordCardinality, RecordFieldSchema, RecordInstance,
            RecordOwnerKind, RecordRrset, RecordTypeDefinition, RecordTypeSchema,
        },
        tasks::{TaskEnvelope, TaskStatus},
        types::{
            CidrValue, CommunityName, DhcpPriority, DnsName, DnsTypeCode, EmailAddressValue,
            Hostname, IpAddressValue, LabelName, MacAddressValue, NetworkPolicyName,
            RecordTypeName, ReservedCount, SerialNumber, SoaSeconds, Ttl, VlanId, ZoneName,
        },
        zone::{ForwardZone, ForwardZoneDelegation, ReverseZone, ReverseZoneDelegation},
    },
    errors::AppError,
};

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::labels)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct LabelRow {
    id: Uuid,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl LabelRow {
    pub fn into_domain(self) -> Result<Label, AppError> {
        Label::restore(
            self.id,
            LabelName::new(self.name)?,
            self.description,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::nameservers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NameServerRow {
    id: Uuid,
    name: String,
    ttl: Option<i32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl NameServerRow {
    pub fn into_domain(self) -> Result<NameServer, AppError> {
        let ttl = self
            .ttl
            .map(|value| {
                Ttl::new(
                    u32::try_from(value)
                        .map_err(|_| AppError::internal("invalid TTL value in database"))?,
                )
            })
            .transpose()?;
        NameServer::restore(
            self.id,
            DnsName::new(self.name)?,
            ttl,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::forward_zones)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ForwardZoneRow {
    id: Uuid,
    name: String,
    updated: bool,
    primary_ns: String,
    email: String,
    serial_no: i64,
    serial_no_updated_at: DateTime<Utc>,
    refresh: i32,
    retry: i32,
    expire: i32,
    soa_ttl: i32,
    default_ttl: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ForwardZoneRow {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn serial_no(&self) -> i64 {
        self.serial_no
    }

    pub fn into_domain(self, nameservers: Vec<DnsName>) -> Result<ForwardZone, AppError> {
        ForwardZone::restore(
            self.id,
            ZoneName::new(self.name)?,
            self.updated,
            DnsName::new(self.primary_ns)?,
            nameservers,
            EmailAddressValue::new(self.email)?,
            SerialNumber::new(
                u64::try_from(self.serial_no)
                    .map_err(|_| AppError::internal("invalid serial number in database"))?,
            )?,
            self.serial_no_updated_at,
            SoaSeconds::new(
                u32::try_from(self.refresh)
                    .map_err(|_| AppError::internal("invalid refresh value in database"))?,
            )?,
            SoaSeconds::new(
                u32::try_from(self.retry)
                    .map_err(|_| AppError::internal("invalid retry value in database"))?,
            )?,
            SoaSeconds::new(
                u32::try_from(self.expire)
                    .map_err(|_| AppError::internal("invalid expire value in database"))?,
            )?,
            Ttl::new(
                u32::try_from(self.soa_ttl)
                    .map_err(|_| AppError::internal("invalid soa_ttl value in database"))?,
            )?,
            Ttl::new(
                u32::try_from(self.default_ttl)
                    .map_err(|_| AppError::internal("invalid default_ttl value in database"))?,
            )?,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct ReverseZoneRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Nullable<Text>)]
    network: Option<String>,
    #[diesel(sql_type = Bool)]
    updated: bool,
    #[diesel(sql_type = Text)]
    primary_ns: String,
    #[diesel(sql_type = Text)]
    email: String,
    #[diesel(sql_type = BigInt)]
    serial_no: i64,
    #[diesel(sql_type = Timestamptz)]
    serial_no_updated_at: DateTime<Utc>,
    #[diesel(sql_type = Integer)]
    refresh: i32,
    #[diesel(sql_type = Integer)]
    retry: i32,
    #[diesel(sql_type = Integer)]
    expire: i32,
    #[diesel(sql_type = Integer)]
    soa_ttl: i32,
    #[diesel(sql_type = Integer)]
    default_ttl: i32,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl ReverseZoneRow {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn serial_no(&self) -> i64 {
        self.serial_no
    }

    pub fn into_domain(self, nameservers: Vec<DnsName>) -> Result<ReverseZone, AppError> {
        let network = self.network.map(CidrValue::new).transpose()?;
        ReverseZone::restore(
            self.id,
            ZoneName::new(self.name)?,
            network,
            self.updated,
            DnsName::new(self.primary_ns)?,
            nameservers,
            EmailAddressValue::new(self.email)?,
            SerialNumber::new(
                u64::try_from(self.serial_no)
                    .map_err(|_| AppError::internal("invalid serial number in database"))?,
            )?,
            self.serial_no_updated_at,
            SoaSeconds::new(
                u32::try_from(self.refresh)
                    .map_err(|_| AppError::internal("invalid refresh value in database"))?,
            )?,
            SoaSeconds::new(
                u32::try_from(self.retry)
                    .map_err(|_| AppError::internal("invalid retry value in database"))?,
            )?,
            SoaSeconds::new(
                u32::try_from(self.expire)
                    .map_err(|_| AppError::internal("invalid expire value in database"))?,
            )?,
            Ttl::new(
                u32::try_from(self.soa_ttl)
                    .map_err(|_| AppError::internal("invalid soa_ttl value in database"))?,
            )?,
            Ttl::new(
                u32::try_from(self.default_ttl)
                    .map_err(|_| AppError::internal("invalid default_ttl value in database"))?,
            )?,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct NetworkRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    network: String,
    #[diesel(sql_type = Text)]
    description: String,
    #[diesel(sql_type = Nullable<Integer>)]
    vlan: Option<i32>,
    #[diesel(sql_type = Bool)]
    dns_delegated: bool,
    #[diesel(sql_type = Text)]
    category: String,
    #[diesel(sql_type = Text)]
    location: String,
    #[diesel(sql_type = Bool)]
    frozen: bool,
    #[diesel(sql_type = Integer)]
    reserved: i32,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl NetworkRow {
    pub fn into_domain(self) -> Result<Network, AppError> {
        Network::restore(
            self.id,
            CidrValue::new(self.network)?,
            self.description,
            self.vlan
                .map(|v| {
                    let v: u32 = v.try_into().map_err(|_| {
                        AppError::internal(format!("invalid vlan value in database: {v}"))
                    })?;
                    VlanId::new(v)
                })
                .transpose()?,
            self.dns_delegated,
            self.category,
            self.location,
            self.frozen,
            ReservedCount::new(u32::try_from(self.reserved).map_err(|_| {
                AppError::internal(format!(
                    "invalid reserved count in database: {}",
                    self.reserved
                ))
            })?)?,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct ExcludedRangeRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    network_id: Uuid,
    #[diesel(sql_type = Text)]
    start_ip: String,
    #[diesel(sql_type = Text)]
    end_ip: String,
    #[diesel(sql_type = Text)]
    description: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl ExcludedRangeRow {
    pub fn into_domain(self) -> Result<ExcludedRange, AppError> {
        ExcludedRange::restore(
            self.id,
            self.network_id,
            IpAddressValue::new(self.start_ip)?,
            IpAddressValue::new(self.end_ip)?,
            self.description,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct HostRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Nullable<Text>)]
    zone_name: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    ttl: Option<i32>,
    #[diesel(sql_type = Text)]
    comment: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl HostRow {
    pub fn into_domain(self) -> Result<Host, AppError> {
        Host::restore(
            self.id,
            Hostname::new(self.name)?,
            self.zone_name.map(ZoneName::new).transpose()?,
            self.ttl
                .map(|value| {
                    Ttl::new(
                        u32::try_from(value)
                            .map_err(|_| AppError::internal("invalid TTL value in database"))?,
                    )
                })
                .transpose()?,
            self.comment,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct HostAttachmentRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    host_id: Uuid,
    #[diesel(sql_type = Text)]
    host_name: String,
    #[diesel(sql_type = SqlUuid)]
    network_id: Uuid,
    #[diesel(sql_type = Text)]
    network_cidr: String,
    #[diesel(sql_type = Nullable<Text>)]
    mac_address: Option<String>,
    #[diesel(sql_type = Nullable<Text>)]
    comment: Option<String>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl HostAttachmentRow {
    pub fn into_domain(self) -> Result<HostAttachment, AppError> {
        Ok(HostAttachment::restore(
            self.id,
            self.host_id,
            Hostname::new(self.host_name)?,
            self.network_id,
            CidrValue::new(self.network_cidr)?,
            self.mac_address.map(MacAddressValue::new).transpose()?,
            self.comment,
            self.created_at,
            self.updated_at,
        ))
    }
}

#[derive(QueryableByName)]
pub struct IpAddressAssignmentRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    host_id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    attachment_id: Uuid,
    #[diesel(sql_type = Text)]
    address: String,
    #[diesel(sql_type = Integer)]
    family: i32,
    #[diesel(sql_type = SqlUuid)]
    network_id: Uuid,
    #[diesel(sql_type = Nullable<Text>)]
    mac_address: Option<String>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl IpAddressAssignmentRow {
    pub fn host_id(&self) -> Uuid {
        self.host_id
    }

    pub fn attachment_id(&self) -> Uuid {
        self.attachment_id
    }

    pub fn family(&self) -> i32 {
        self.family
    }

    pub fn address_str(&self) -> String {
        self.address.clone()
    }

    pub fn into_domain(self) -> Result<IpAddressAssignment, AppError> {
        let assignment = IpAddressAssignment::restore(
            self.id,
            self.host_id,
            self.attachment_id,
            IpAddressValue::new(self.address)?,
            self.network_id,
            self.mac_address.map(MacAddressValue::new).transpose()?,
            self.created_at,
            self.updated_at,
        )?;

        if assignment.family() as i32 != self.family {
            return Err(AppError::internal(
                "stored IP family does not match address family",
            ));
        }

        Ok(assignment)
    }
}

#[derive(QueryableByName)]
pub struct AttachmentDhcpIdentifierRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    attachment_id: Uuid,
    #[diesel(sql_type = Integer)]
    family: i32,
    #[diesel(sql_type = Text)]
    kind: String,
    #[diesel(sql_type = Text)]
    value: String,
    #[diesel(sql_type = Integer)]
    priority: i32,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl AttachmentDhcpIdentifierRow {
    pub fn into_domain(self) -> Result<AttachmentDhcpIdentifier, AppError> {
        AttachmentDhcpIdentifier::restore(
            self.id,
            self.attachment_id,
            match self.family {
                4 => DhcpIdentifierFamily::V4,
                6 => DhcpIdentifierFamily::V6,
                _ => return Err(AppError::internal("invalid stored dhcp identifier family")),
            },
            match self.kind.as_str() {
                "client_id" => DhcpIdentifierKind::ClientId,
                "duid_llt" => DhcpIdentifierKind::DuidLlt,
                "duid_en" => DhcpIdentifierKind::DuidEn,
                "duid_ll" => DhcpIdentifierKind::DuidLl,
                "duid_uuid" => DhcpIdentifierKind::DuidUuid,
                "duid_raw" => DhcpIdentifierKind::DuidRaw,
                _ => return Err(AppError::internal("invalid stored dhcp identifier kind")),
            },
            self.value,
            DhcpPriority::new(self.priority),
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct AttachmentPrefixReservationRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    attachment_id: Uuid,
    #[diesel(sql_type = Text)]
    prefix: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl AttachmentPrefixReservationRow {
    pub fn into_domain(self) -> Result<AttachmentPrefixReservation, AppError> {
        AttachmentPrefixReservation::restore(
            self.id,
            self.attachment_id,
            CidrValue::new(self.prefix)?,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(QueryableByName)]
pub struct AttachmentCommunityAssignmentRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    attachment_id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    host_id: Uuid,
    #[diesel(sql_type = Text)]
    host_name: String,
    #[diesel(sql_type = SqlUuid)]
    network_id: Uuid,
    #[diesel(sql_type = Text)]
    network_cidr: String,
    #[diesel(sql_type = SqlUuid)]
    community_id: Uuid,
    #[diesel(sql_type = Text)]
    community_name: String,
    #[diesel(sql_type = Text)]
    policy_name: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl AttachmentCommunityAssignmentRow {
    pub fn into_domain(self) -> Result<AttachmentCommunityAssignment, AppError> {
        Ok(AttachmentCommunityAssignment::restore(
            self.id,
            self.attachment_id,
            self.host_id,
            Hostname::new(self.host_name)?,
            self.network_id,
            CidrValue::new(self.network_cidr)?,
            self.community_id,
            CommunityName::new(self.community_name)?,
            NetworkPolicyName::new(self.policy_name)?,
            self.created_at,
            self.updated_at,
        ))
    }
}

#[derive(Queryable, Selectable, QueryableByName)]
#[diesel(table_name = crate::db::schema::tasks)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TaskRow {
    #[diesel(sql_type = diesel::sql_types::Uuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    kind: String,
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    payload: serde_json::Value,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    progress: serde_json::Value,
    #[diesel(sql_type = Nullable<diesel::sql_types::Jsonb>)]
    result: Option<serde_json::Value>,
    #[diesel(sql_type = Nullable<Text>)]
    error_summary: Option<String>,
    #[diesel(sql_type = Integer)]
    attempts: i32,
    #[diesel(sql_type = Integer)]
    max_attempts: i32,
    #[diesel(sql_type = Timestamptz)]
    available_at: DateTime<Utc>,
    #[diesel(sql_type = Nullable<Timestamptz>)]
    started_at: Option<DateTime<Utc>>,
    #[diesel(sql_type = Nullable<Timestamptz>)]
    finished_at: Option<DateTime<Utc>>,
}

impl TaskRow {
    pub fn into_domain(self) -> Result<TaskEnvelope, AppError> {
        TaskEnvelope::restore(
            self.id,
            self.kind,
            parse_task_status(&self.status)?,
            self.payload,
            self.progress,
            self.result,
            self.error_summary,
            self.attempts,
            self.max_attempts,
            self.available_at,
            self.started_at,
            self.finished_at,
        )
    }
}

#[derive(Queryable, Selectable, QueryableByName)]
#[diesel(table_name = crate::db::schema::imports)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ImportBatchRow {
    #[diesel(sql_type = diesel::sql_types::Uuid)]
    id: Uuid,
    #[diesel(sql_type = Nullable<diesel::sql_types::Uuid>)]
    task_id: Option<Uuid>,
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = Nullable<Text>)]
    requested_by: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    batch: serde_json::Value,
    #[diesel(sql_type = Nullable<diesel::sql_types::Jsonb>)]
    validation_report: Option<serde_json::Value>,
    #[diesel(sql_type = Nullable<diesel::sql_types::Jsonb>)]
    commit_summary: Option<serde_json::Value>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl ImportBatchRow {
    pub fn into_summary(self) -> Result<ImportBatchSummary, AppError> {
        Ok(ImportBatchSummary::restore(
            self.id,
            self.task_id,
            parse_import_status(&self.status)?,
            self.requested_by,
            self.validation_report,
            self.commit_summary,
            self.created_at,
            self.updated_at,
        ))
    }

    pub fn into_batch(self) -> Result<ImportBatch, AppError> {
        serde_json::from_value(self.batch).map_err(AppError::internal)
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::export_templates)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ExportTemplateRow {
    id: Uuid,
    name: String,
    description: String,
    engine: String,
    scope: String,
    body: String,
    metadata: serde_json::Value,
    built_in: bool,
}

impl ExportTemplateRow {
    pub fn into_domain(self) -> Result<ExportTemplate, AppError> {
        ExportTemplate::restore(
            self.id,
            self.name,
            self.description,
            self.engine,
            self.scope,
            self.body,
            self.metadata,
            self.built_in,
        )
    }
}

#[derive(Queryable, Selectable, QueryableByName)]
#[diesel(table_name = crate::db::schema::export_runs)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ExportRunRow {
    #[diesel(sql_type = diesel::sql_types::Uuid)]
    id: Uuid,
    #[diesel(sql_type = Nullable<diesel::sql_types::Uuid>)]
    task_id: Option<Uuid>,
    #[diesel(sql_type = Nullable<diesel::sql_types::Uuid>)]
    template_id: Option<Uuid>,
    #[diesel(sql_type = Nullable<Text>)]
    requested_by: Option<String>,
    #[diesel(sql_type = Text)]
    scope: String,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    parameters: serde_json::Value,
    #[diesel(sql_type = Text)]
    status: String,
    #[diesel(sql_type = Nullable<Text>)]
    rendered_output: Option<String>,
    #[diesel(sql_type = Nullable<diesel::sql_types::Jsonb>)]
    artifact_metadata: Option<serde_json::Value>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl ExportRunRow {
    pub fn into_domain(self) -> Result<ExportRun, AppError> {
        ExportRun::restore(
            self.id,
            self.task_id,
            self.template_id,
            self.requested_by,
            self.scope,
            self.parameters,
            parse_export_status(&self.status)?,
            self.rendered_output,
            self.artifact_metadata,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::record_types)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RecordTypeRow {
    id: Uuid,
    name: String,
    dns_type: Option<i32>,
    owner_kind: String,
    cardinality: String,
    validation_schema: serde_json::Value,
    rendering_schema: serde_json::Value,
    behavior_flags: serde_json::Value,
    built_in: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RecordTypeRow {
    pub fn into_domain(self) -> Result<RecordTypeDefinition, AppError> {
        Ok(RecordTypeDefinition::restore(
            self.id,
            RecordTypeName::new(self.name)?,
            self.dns_type.map(DnsTypeCode::new).transpose()?,
            record_type_schema_from_parts(
                &self.owner_kind,
                &self.cardinality,
                self.validation_schema,
                self.rendering_schema,
                self.behavior_flags,
            )?,
            self.built_in,
            self.created_at,
            self.updated_at,
        ))
    }
}

#[derive(QueryableByName)]
pub struct RrsetRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    type_id: Uuid,
    #[diesel(sql_type = Text)]
    type_name: String,
    #[diesel(sql_type = Text)]
    dns_class: String,
    #[diesel(sql_type = Text)]
    owner_name: String,
    #[diesel(sql_type = Nullable<Text>)]
    anchor_kind: Option<String>,
    #[diesel(sql_type = Nullable<SqlUuid>)]
    anchor_id: Option<Uuid>,
    #[diesel(sql_type = Nullable<Text>)]
    anchor_name: Option<String>,
    #[diesel(sql_type = Nullable<SqlUuid>)]
    zone_id: Option<Uuid>,
    #[diesel(sql_type = Nullable<Integer>)]
    ttl: Option<i32>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl RrsetRow {
    pub fn into_domain(self) -> Result<RecordRrset, AppError> {
        Ok(RecordRrset::restore(
            self.id,
            self.type_id,
            RecordTypeName::new(self.type_name)?,
            parse_dns_class(&self.dns_class)?,
            DnsName::new(self.owner_name)?,
            self.anchor_kind
                .as_deref()
                .map(parse_record_owner_kind)
                .transpose()?,
            self.anchor_id,
            self.anchor_name,
            self.zone_id,
            self.ttl
                .map(|value| {
                    Ttl::new(
                        u32::try_from(value)
                            .map_err(|_| AppError::internal("invalid TTL value in database"))?,
                    )
                })
                .transpose()?,
            self.created_at,
            self.updated_at,
        ))
    }
}

#[derive(QueryableByName)]
pub struct RecordRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    rrset_id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    type_id: Uuid,
    #[diesel(sql_type = Text)]
    type_name: String,
    #[diesel(sql_type = Nullable<Text>)]
    anchor_kind: Option<String>,
    #[diesel(sql_type = Nullable<SqlUuid>)]
    anchor_id: Option<Uuid>,
    #[diesel(sql_type = Text)]
    owner_name: String,
    #[diesel(sql_type = Nullable<SqlUuid>)]
    zone_id: Option<Uuid>,
    #[diesel(sql_type = Nullable<Integer>)]
    ttl: Option<i32>,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    data: serde_json::Value,
    #[diesel(sql_type = Nullable<Bytea>)]
    raw_rdata: Option<Vec<u8>>,
    #[diesel(sql_type = Nullable<Text>)]
    rendered: Option<String>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl RecordRow {
    pub fn into_domain(self) -> Result<RecordInstance, AppError> {
        Ok(RecordInstance::restore(
            self.id,
            self.rrset_id,
            self.type_id,
            RecordTypeName::new(self.type_name)?,
            self.anchor_kind
                .as_deref()
                .map(parse_record_owner_kind)
                .transpose()?,
            self.anchor_id,
            DnsName::new(self.owner_name)?,
            self.zone_id,
            self.ttl
                .map(|value| {
                    Ttl::new(
                        u32::try_from(value)
                            .map_err(|_| AppError::internal("invalid TTL value in database"))?,
                    )
                })
                .transpose()?,
            self.data,
            self.raw_rdata
                .map(RawRdataValue::from_wire_bytes)
                .transpose()?,
            self.rendered,
            self.created_at,
            self.updated_at,
        ))
    }
}

#[derive(QueryableByName)]
pub struct NameRow {
    #[diesel(sql_type = Text)]
    name: String,
}

impl NameRow {
    pub fn into_dns_name(self) -> Result<DnsName, AppError> {
        DnsName::new(self.name)
    }
}

#[derive(QueryableByName)]
pub struct UuidRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
}

impl UuidRow {
    pub fn id(&self) -> Uuid {
        self.id
    }
}

#[derive(QueryableByName)]
pub struct SeededRecordTypeRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Nullable<Integer>)]
    dns_type: Option<i32>,
    #[diesel(sql_type = Text)]
    owner_kind: String,
    #[diesel(sql_type = Text)]
    cardinality: String,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    validation_schema: serde_json::Value,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    rendering_schema: serde_json::Value,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    behavior_flags: serde_json::Value,
    #[diesel(sql_type = Bool)]
    built_in: bool,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

impl SeededRecordTypeRow {
    #[allow(clippy::type_complexity)]
    pub fn into_tuple(
        self,
    ) -> (
        Uuid,
        String,
        Option<i32>,
        String,
        String,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        bool,
        DateTime<Utc>,
        DateTime<Utc>,
    ) {
        (
            self.id,
            self.name,
            self.dns_type,
            self.owner_kind,
            self.cardinality,
            self.validation_schema,
            self.rendering_schema,
            self.behavior_flags,
            self.built_in,
            self.created_at,
            self.updated_at,
        )
    }
}

fn parse_task_status(value: &str) -> Result<TaskStatus, AppError> {
    match value {
        "queued" => Ok(TaskStatus::Queued),
        "running" => Ok(TaskStatus::Running),
        "succeeded" => Ok(TaskStatus::Succeeded),
        "failed" => Ok(TaskStatus::Failed),
        "cancelled" => Ok(TaskStatus::Cancelled),
        _ => Err(AppError::internal(format!("unknown task status '{value}'"))),
    }
}

fn parse_import_status(value: &str) -> Result<ImportBatchStatus, AppError> {
    match value {
        "queued" => Ok(ImportBatchStatus::Queued),
        "validating" => Ok(ImportBatchStatus::Validating),
        "ready" => Ok(ImportBatchStatus::Ready),
        "committing" => Ok(ImportBatchStatus::Committing),
        "succeeded" => Ok(ImportBatchStatus::Succeeded),
        "failed" => Ok(ImportBatchStatus::Failed),
        "cancelled" => Ok(ImportBatchStatus::Cancelled),
        _ => Err(AppError::internal(format!(
            "unknown import status '{value}'"
        ))),
    }
}

fn parse_export_status(value: &str) -> Result<ExportRunStatus, AppError> {
    match value {
        "queued" => Ok(ExportRunStatus::Queued),
        "running" => Ok(ExportRunStatus::Running),
        "succeeded" => Ok(ExportRunStatus::Succeeded),
        "failed" => Ok(ExportRunStatus::Failed),
        "cancelled" => Ok(ExportRunStatus::Cancelled),
        _ => Err(AppError::internal(format!(
            "unknown export status '{value}'"
        ))),
    }
}

fn parse_record_owner_kind(value: &str) -> Result<RecordOwnerKind, AppError> {
    match value {
        "host" => Ok(RecordOwnerKind::Host),
        "forward_zone" => Ok(RecordOwnerKind::ForwardZone),
        "forward_zone_delegation" => Ok(RecordOwnerKind::ForwardZoneDelegation),
        "reverse_zone" => Ok(RecordOwnerKind::ReverseZone),
        "reverse_zone_delegation" => Ok(RecordOwnerKind::ReverseZoneDelegation),
        "nameserver" => Ok(RecordOwnerKind::NameServer),
        _ => Err(AppError::internal(format!(
            "unknown record owner kind '{value}'"
        ))),
    }
}

fn parse_dns_class(value: &str) -> Result<DnsClass, AppError> {
    match value {
        "IN" => Ok(DnsClass::IN),
        _ => Err(AppError::internal(format!("unknown dns class '{value}'"))),
    }
}

fn parse_record_cardinality(value: &str) -> Result<RecordCardinality, AppError> {
    match value {
        "single" => Ok(RecordCardinality::Single),
        "multiple" => Ok(RecordCardinality::Multiple),
        _ => Err(AppError::internal(format!(
            "unknown record cardinality '{value}'"
        ))),
    }
}

fn record_type_schema_from_parts(
    owner_kind: &str,
    cardinality: &str,
    validation_schema: serde_json::Value,
    rendering_schema: serde_json::Value,
    behavior_flags: serde_json::Value,
) -> Result<RecordTypeSchema, AppError> {
    let zone_bound = validation_schema
        .get("zone_bound")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let fields_value = validation_schema
        .get("fields")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let fields: Vec<RecordFieldSchema> =
        serde_json::from_value(fields_value).map_err(AppError::internal)?;
    let render_template = rendering_schema
        .get("render_template")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    RecordTypeSchema::new(
        parse_record_owner_kind(owner_kind)?,
        parse_record_cardinality(cardinality)?,
        zone_bound,
        fields,
        behavior_flags,
        render_template,
    )
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::history_events)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct HistoryEventRow {
    id: Uuid,
    actor: String,
    resource_kind: String,
    resource_id: Option<Uuid>,
    resource_name: String,
    action: String,
    data: serde_json::Value,
    created_at: DateTime<Utc>,
}

impl HistoryEventRow {
    pub fn into_domain(self) -> HistoryEvent {
        HistoryEvent::restore(
            self.id,
            self.actor,
            self.resource_kind,
            self.resource_id,
            self.resource_name,
            self.action,
            self.data,
            self.created_at,
        )
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::forward_zone_delegations)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ForwardDelegationRow {
    id: Uuid,
    zone_id: Uuid,
    name: String,
    comment: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ForwardDelegationRow {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn into_forward_delegation(
        self,
        nameservers: Vec<DnsName>,
    ) -> Result<ForwardZoneDelegation, AppError> {
        ForwardZoneDelegation::restore(
            self.id,
            self.zone_id,
            DnsName::new(self.name)?,
            self.comment,
            nameservers,
            self.created_at,
            self.updated_at,
        )
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::db::schema::reverse_zone_delegations)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct ReverseDelegationRow {
    id: Uuid,
    zone_id: Uuid,
    name: String,
    comment: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ReverseDelegationRow {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn into_reverse_delegation(
        self,
        nameservers: Vec<DnsName>,
    ) -> Result<ReverseZoneDelegation, AppError> {
        ReverseZoneDelegation::restore(
            self.id,
            self.zone_id,
            DnsName::new(self.name)?,
            self.comment,
            nameservers,
            self.created_at,
            self.updated_at,
        )
    }
}
