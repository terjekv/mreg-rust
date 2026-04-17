use std::collections::BTreeMap;

use diesel::{
    OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Bytea, Integer, Nullable, Text, Uuid as SqlUuid},
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    audit::HistoryEvent,
    domain::{
        attachment::{AttachmentCommunityAssignment, HostAttachment},
        community::Community,
        exports::{ExportRun, ExportTemplate},
        host::{Host, IpAddressAssignment},
        host_community_assignment::HostCommunityAssignment,
        host_contact::HostContact,
        host_group::HostGroup,
        host_policy::{HostPolicyAtom, HostPolicyRole},
        imports::ImportBatchSummary,
        label::Label,
        nameserver::NameServer,
        network::{ExcludedRange, Network},
        network_policy::NetworkPolicy,
        pagination::{Page, PageRequest},
        ptr_override::PtrOverride,
        resource_records::{
            ExistingRecordSummary, RawRdataValue, RecordCardinality, RecordInstance,
            RecordOwnerKind, RecordRrset, RecordTypeDefinition, RecordTypeSchema,
        },
        tasks::TaskEnvelope,
        types::{DnsName, RecordTypeName, Ttl},
        zone::{ForwardZone, ForwardZoneDelegation, ReverseZone, ReverseZoneDelegation},
    },
    errors::AppError,
};

use super::PostgresStorage;

/// Execute a dynamically-built SQL query with string bind values.
/// Returns the loaded rows for any `QueryableByName` type.
pub(super) fn run_dynamic_query<T: QueryableByName<diesel::pg::Pg> + 'static>(
    connection: &mut PgConnection,
    query_str: &str,
    values: &[String],
) -> Result<Vec<T>, diesel::result::Error> {
    // We need to handle 0..N bind parameters. Since each `.bind()` changes
    // the type in Diesel, we match on the count and bind all at once.
    // Support up to 12 bind params which is plenty for filter queries.
    match values.len() {
        0 => sql_query(query_str).load::<T>(connection),
        1 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .load::<T>(connection),
        2 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .load::<T>(connection),
        3 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .load::<T>(connection),
        4 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .load::<T>(connection),
        5 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .load::<T>(connection),
        6 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .load::<T>(connection),
        7 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .bind::<Text, _>(&values[6])
            .load::<T>(connection),
        8 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .bind::<Text, _>(&values[6])
            .bind::<Text, _>(&values[7])
            .load::<T>(connection),
        9 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .bind::<Text, _>(&values[6])
            .bind::<Text, _>(&values[7])
            .bind::<Text, _>(&values[8])
            .load::<T>(connection),
        10 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .bind::<Text, _>(&values[6])
            .bind::<Text, _>(&values[7])
            .bind::<Text, _>(&values[8])
            .bind::<Text, _>(&values[9])
            .load::<T>(connection),
        11 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .bind::<Text, _>(&values[6])
            .bind::<Text, _>(&values[7])
            .bind::<Text, _>(&values[8])
            .bind::<Text, _>(&values[9])
            .bind::<Text, _>(&values[10])
            .load::<T>(connection),
        12 => sql_query(query_str)
            .bind::<Text, _>(&values[0])
            .bind::<Text, _>(&values[1])
            .bind::<Text, _>(&values[2])
            .bind::<Text, _>(&values[3])
            .bind::<Text, _>(&values[4])
            .bind::<Text, _>(&values[5])
            .bind::<Text, _>(&values[6])
            .bind::<Text, _>(&values[7])
            .bind::<Text, _>(&values[8])
            .bind::<Text, _>(&values[9])
            .bind::<Text, _>(&values[10])
            .bind::<Text, _>(&values[11])
            .load::<T>(connection),
        n => Err(diesel::result::Error::QueryBuilderError(
            format!("too many filter bind parameters ({n}), max supported is 12").into(),
        )),
    }
}

pub(super) fn vec_to_page<T: HasId>(items: Vec<T>, page: &PageRequest) -> Page<T> {
    vec_to_page_with_cursor(items, page)
}

pub(super) fn paginate_simple<T>(items: Vec<T>, page: &PageRequest) -> Page<T> {
    let total = items.len() as u64;
    let limit = page.limit() as usize;
    let page_items: Vec<T> = items.into_iter().take(limit).collect();
    Page {
        items: page_items,
        total,
        next_cursor: None,
    }
}

pub(super) trait HasId {
    fn id(&self) -> Uuid;
}

macro_rules! impl_has_id {
    ($($type:ty),*$(,)?) => {
        $(
            impl HasId for $type {
                fn id(&self) -> Uuid {
                    self.id()
                }
            }
        )*
    };
}

impl_has_id!(
    HostPolicyAtom,
    HostPolicyRole,
    Label,
    NameServer,
    ForwardZone,
    ReverseZone,
    ForwardZoneDelegation,
    ReverseZoneDelegation,
    Network,
    ExcludedRange,
    Host,
    IpAddressAssignment,
    HostContact,
    HostGroup,
    PtrOverride,
    NetworkPolicy,
    Community,
    HostAttachment,
    AttachmentCommunityAssignment,
    HostCommunityAssignment,
    TaskEnvelope,
    ImportBatchSummary,
    ExportTemplate,
    ExportRun,
    RecordTypeDefinition,
    RecordRrset,
    RecordInstance,
    HistoryEvent,
);

fn vec_to_page_with_cursor<T: HasId>(items: Vec<T>, page: &PageRequest) -> Page<T> {
    let total = items.len() as u64;
    let start = if let Some(cursor) = page.after() {
        items
            .iter()
            .position(|item| item.id() == cursor)
            .map(|position| position + 1)
            .unwrap_or(0)
    } else {
        0
    };
    let limit = page.limit() as usize;
    let take_count = limit.saturating_add(1);
    let mut page_items: Vec<T> = items.into_iter().skip(start).take(take_count).collect();
    let has_more = page_items.len() > limit;
    if has_more {
        page_items.pop();
    }
    Page {
        next_cursor: if has_more {
            page_items.last().map(|item| item.id())
        } else {
            None
        },
        items: page_items,
        total,
    }
}

pub(super) fn map_unique(message: &'static str) -> impl FnOnce(diesel::result::Error) -> AppError {
    move |error| match error {
        diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        ) => AppError::conflict(message),
        other => AppError::internal(other),
    }
}

pub(super) fn record_owner_kind_value(kind: &RecordOwnerKind) -> &'static str {
    match kind {
        RecordOwnerKind::Host => "host",
        RecordOwnerKind::ForwardZone => "forward_zone",
        RecordOwnerKind::ForwardZoneDelegation => "forward_zone_delegation",
        RecordOwnerKind::ReverseZone => "reverse_zone",
        RecordOwnerKind::ReverseZoneDelegation => "reverse_zone_delegation",
        RecordOwnerKind::NameServer => "nameserver",
    }
}

pub(super) fn record_cardinality_value(cardinality: &RecordCardinality) -> &'static str {
    match cardinality {
        RecordCardinality::Single => "single",
        RecordCardinality::Multiple => "multiple",
    }
}

pub(super) fn record_type_storage_parts(
    schema: &RecordTypeSchema,
) -> (String, String, Value, Value, Value) {
    (
        record_owner_kind_value(schema.owner_kind()).to_string(),
        record_cardinality_value(schema.cardinality()).to_string(),
        serde_json::json!({
            "zone_bound": schema.zone_bound(),
            "fields": schema.fields(),
        }),
        serde_json::json!({
            "render_template": schema.render_template(),
        }),
        schema.behavior_flags().clone(),
    )
}

#[derive(QueryableByName)]
pub(super) struct TextValueRow {
    #[diesel(sql_type = Text)]
    pub value: String,
}

#[derive(QueryableByName)]
pub(super) struct ExistingRecordRow {
    #[diesel(sql_type = Text)]
    pub type_name: String,
    #[diesel(sql_type = Nullable<Integer>)]
    pub ttl: Option<i32>,
    #[diesel(sql_type = diesel::sql_types::Jsonb)]
    pub data: serde_json::Value,
    #[diesel(sql_type = Nullable<Bytea>)]
    pub raw_rdata: Option<Vec<u8>>,
}

impl ExistingRecordRow {
    pub fn into_summary(self) -> Result<ExistingRecordSummary, AppError> {
        Ok(ExistingRecordSummary::new(
            RecordTypeName::new(self.type_name)?,
            self.ttl.map(|value| Ttl::new(value as u32)).transpose()?,
            self.data,
            self.raw_rdata
                .map(RawRdataValue::from_wire_bytes)
                .transpose()?,
        ))
    }
}

#[allow(dead_code)]
#[derive(QueryableByName)]
pub(super) struct IntSentinelRow {
    #[diesel(sql_type = Integer)]
    pub value: i32,
}

#[derive(QueryableByName)]
pub(super) struct NameAndIdRow {
    #[diesel(sql_type = SqlUuid)]
    pub id: Uuid,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    pub name: String,
}

// Shared impl PostgresStorage helpers used by multiple stores
impl PostgresStorage {
    /// Best-effort zone serial bump within an existing transaction.
    /// Tries forward_zones first, then reverse_zones. Silently ignores errors.
    pub(super) fn bump_zone_serial_tx(connection: &mut PgConnection, zone_id: Uuid) {
        use crate::db::schema::{forward_zones, reverse_zones};
        use diesel::{ExpressionMethods, QueryDsl};

        // Try forward zone first
        let _ = diesel::update(forward_zones::table.filter(forward_zones::id.eq(zone_id)))
            .set((
                forward_zones::serial_no.eq(forward_zones::serial_no + 1i64),
                forward_zones::serial_no_updated_at.eq(diesel::dsl::now),
                forward_zones::updated.eq(true),
                forward_zones::updated_at.eq(diesel::dsl::now),
            ))
            .execute(connection)
            .ok()
            .and_then(|updated| {
                if updated == 0 {
                    // Try reverse zone
                    diesel::update(reverse_zones::table.filter(reverse_zones::id.eq(zone_id)))
                        .set((
                            reverse_zones::serial_no.eq(reverse_zones::serial_no + 1i64),
                            reverse_zones::serial_no_updated_at.eq(diesel::dsl::now),
                            reverse_zones::updated.eq(true),
                            reverse_zones::updated_at.eq(diesel::dsl::now),
                        ))
                        .execute(connection)
                        .ok()
                } else {
                    Some(updated)
                }
            });
    }

    pub(super) fn lookup_nameserver_ids(
        connection: &mut PgConnection,
        names: &[DnsName],
    ) -> Result<Vec<Uuid>, AppError> {
        use crate::db::schema::nameservers;
        use diesel::{ExpressionMethods, QueryDsl};

        let mut ids = Vec::with_capacity(names.len());
        for name in names {
            let id = nameservers::table
                .filter(nameservers::name.eq(name.as_str()))
                .select(nameservers::id)
                .first::<Uuid>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("nameserver '{}' does not exist", name.as_str()))
                })?;
            ids.push(id);
        }
        Ok(ids)
    }

    pub(super) fn ensure_builtin_record_types(
        connection: &mut PgConnection,
    ) -> Result<(), AppError> {
        use crate::db::schema::record_types;
        use crate::domain::resource_records::built_in_record_types;
        use diesel::ExpressionMethods;
        for command in built_in_record_types()? {
            let (owner_kind, cardinality, validation_schema, rendering_schema, behavior_flags) =
                record_type_storage_parts(command.schema());
            diesel::insert_into(record_types::table)
                .values((
                    record_types::name.eq(command.name().as_str()),
                    record_types::dns_type.eq(command.dns_type()),
                    record_types::owner_kind.eq(&owner_kind),
                    record_types::cardinality.eq(&cardinality),
                    record_types::validation_schema.eq(&validation_schema),
                    record_types::rendering_schema.eq(&rendering_schema),
                    record_types::behavior_flags.eq(&behavior_flags),
                    record_types::built_in.eq(true),
                ))
                .on_conflict(record_types::name)
                .do_nothing()
                .execute(connection)?;
        }
        Self::ensure_builtin_export_templates(connection)?;
        Ok(())
    }

    pub(super) fn ensure_builtin_export_templates(
        connection: &mut PgConnection,
    ) -> Result<(), AppError> {
        use crate::db::schema::export_templates;
        use crate::domain::builtin_export_templates::built_in_export_templates;
        use diesel::ExpressionMethods;
        for (command, _built_in) in built_in_export_templates()? {
            diesel::insert_into(export_templates::table)
                .values((
                    export_templates::name.eq(command.name()),
                    export_templates::description.eq(command.description()),
                    export_templates::engine.eq(command.engine()),
                    export_templates::scope.eq(command.scope()),
                    export_templates::body.eq(command.body()),
                    export_templates::metadata.eq(command.metadata()),
                    export_templates::built_in.eq(true),
                ))
                .on_conflict(export_templates::name)
                .do_nothing()
                .execute(connection)?;
        }
        Ok(())
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn resolve_record_owner(
        connection: &mut PgConnection,
        owner_kind: Option<&RecordOwnerKind>,
        anchor_name: Option<&str>,
        owner_name: &DnsName,
    ) -> Result<(Option<Uuid>, Option<String>, Option<Uuid>), AppError> {
        use crate::db::schema::{
            forward_zone_delegations, forward_zones, hosts, nameservers, reverse_zone_delegations,
            reverse_zones,
        };
        use diesel::{ExpressionMethods, QueryDsl};

        let Some(owner_kind) = owner_kind else {
            return Ok((
                None,
                None,
                Self::best_matching_zone_for_owner_name(connection, owner_name)?,
            ));
        };

        let anchor_name = anchor_name.unwrap_or(owner_name.as_str());
        match owner_kind {
            RecordOwnerKind::Host => {
                let row = hosts::table
                    .filter(hosts::name.eq(anchor_name))
                    .select((hosts::id, hosts::name, hosts::zone_id))
                    .first::<(Uuid, String, Option<Uuid>)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("host '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), row.2))
            }
            RecordOwnerKind::ForwardZone => {
                let row = forward_zones::table
                    .filter(forward_zones::name.eq(anchor_name))
                    .select((forward_zones::id, forward_zones::name))
                    .first::<(Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("forward zone '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), Some(row.0)))
            }
            RecordOwnerKind::ReverseZone => {
                let row = reverse_zones::table
                    .filter(reverse_zones::name.eq(anchor_name))
                    .select((reverse_zones::id, reverse_zones::name))
                    .first::<(Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("reverse zone '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), Some(row.0)))
            }
            RecordOwnerKind::NameServer => {
                let row = nameservers::table
                    .filter(nameservers::name.eq(anchor_name))
                    .select((nameservers::id, nameservers::name))
                    .first::<(Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("nameserver '{}' was not found", anchor_name))
                    })?;
                Ok((Some(row.0), Some(row.1), None))
            }
            RecordOwnerKind::ForwardZoneDelegation => {
                let row = forward_zone_delegations::table
                    .filter(forward_zone_delegations::name.eq(anchor_name))
                    .select((
                        forward_zone_delegations::id,
                        forward_zone_delegations::zone_id,
                        forward_zone_delegations::name,
                    ))
                    .first::<(Uuid, Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "forward zone delegation '{}' was not found",
                            anchor_name
                        ))
                    })?;
                let owner = owner_name.as_str();
                if owner != row.2 && !owner.ends_with(&format!(".{}", row.2)) {
                    return Err(AppError::validation(format!(
                        "owner name '{}' is not within delegation '{}'",
                        owner, row.2
                    )));
                }
                Ok((Some(row.0), Some(row.2), Some(row.1)))
            }
            RecordOwnerKind::ReverseZoneDelegation => {
                let row = reverse_zone_delegations::table
                    .filter(reverse_zone_delegations::name.eq(anchor_name))
                    .select((
                        reverse_zone_delegations::id,
                        reverse_zone_delegations::zone_id,
                        reverse_zone_delegations::name,
                    ))
                    .first::<(Uuid, Uuid, String)>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!(
                            "reverse zone delegation '{}' was not found",
                            anchor_name
                        ))
                    })?;
                let owner = owner_name.as_str();
                if owner != row.2 && !owner.ends_with(&format!(".{}", row.2)) {
                    return Err(AppError::validation(format!(
                        "owner name '{}' is not within delegation '{}'",
                        owner, row.2
                    )));
                }
                Ok((Some(row.0), Some(row.2), Some(row.1)))
            }
        }
    }

    pub(super) fn best_matching_zone_for_owner_name(
        connection: &mut PgConnection,
        owner_name: &DnsName,
    ) -> Result<Option<Uuid>, AppError> {
        let candidate = owner_name.as_str();
        let rows = sql_query(
            "SELECT id, name::text AS name
             FROM (
                SELECT id, name FROM forward_zones
                UNION ALL
                SELECT id, name FROM reverse_zones
             ) zones
             WHERE lower($1::text) = lower(name::text)
                OR lower($1::text) LIKE '%.' || lower(name::text)
             ORDER BY length(name::text) DESC",
        )
        .bind::<Text, _>(candidate)
        .load::<NameAndIdRow>(connection)?;

        Ok(rows.into_iter().next().map(|row| row.id))
    }

    pub(super) fn render_record_data(
        template: Option<&str>,
        data: &Value,
    ) -> Result<Option<String>, AppError> {
        let Some(template) = template else {
            return Ok(None);
        };
        let mut env = minijinja::Environment::new();
        env.add_template("record", template)
            .map_err(AppError::internal)?;
        Ok(Some(
            env.get_template("record")
                .map_err(AppError::internal)?
                .render(minijinja::value::Value::from_serialize(data))
                .map_err(AppError::internal)?,
        ))
    }

    pub(super) fn query_existing_owner_records(
        connection: &mut PgConnection,
        owner_name: &DnsName,
    ) -> Result<Vec<ExistingRecordSummary>, AppError> {
        let rows = sql_query(
            "SELECT rt.name::text AS type_name, rs.ttl, r.data, r.raw_rdata
             FROM records r
             JOIN rrsets rs ON rs.id = r.rrset_id
             JOIN record_types rt ON rt.id = rs.type_id
             WHERE rs.owner_name = $1
             ORDER BY r.created_at",
        )
        .bind::<Text, _>(owner_name.as_str())
        .load::<ExistingRecordRow>(connection)?;

        rows.into_iter()
            .map(ExistingRecordRow::into_summary)
            .collect()
    }

    pub(super) fn query_existing_rrset_records(
        connection: &mut PgConnection,
        rrset_id: Uuid,
    ) -> Result<Vec<ExistingRecordSummary>, AppError> {
        let rows = sql_query(
            "SELECT rt.name::text AS type_name, rs.ttl, r.data, r.raw_rdata
             FROM records r
             JOIN rrsets rs ON rs.id = r.rrset_id
             JOIN record_types rt ON rt.id = rs.type_id
             WHERE r.rrset_id = $1
             ORDER BY r.created_at",
        )
        .bind::<SqlUuid, _>(rrset_id)
        .load::<ExistingRecordRow>(connection)?;

        rows.into_iter()
            .map(ExistingRecordRow::into_summary)
            .collect()
    }

    pub(super) fn query_alias_owner_names(
        connection: &mut PgConnection,
        names: &[String],
    ) -> Result<BTreeMap<String, bool>, AppError> {
        let mut result = BTreeMap::new();
        for name in names {
            let alias = sql_query(
                "SELECT 1 AS value
                 FROM records r
                 JOIN rrsets rs ON rs.id = r.rrset_id
                 JOIN record_types rt ON rt.id = rs.type_id
                 WHERE rt.name = 'CNAME'
                   AND rs.owner_name = $1
                 LIMIT 1",
            )
            .bind::<Text, _>(name)
            .get_result::<IntSentinelRow>(connection)
            .optional()?
            .is_some();
            result.insert(name.clone(), alias);
        }
        Ok(result)
    }
}
