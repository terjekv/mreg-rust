use std::collections::BTreeMap;

use diesel::{
    OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Bytea, Integer, Nullable, Text, Uuid as SqlUuid},
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::resource_records::{
        ExistingRecordSummary, RawRdataValue, RecordCardinality, RecordOwnerKind, RecordTypeSchema,
        render_record_data as render_rdata,
    },
    domain::types::{DnsName, RecordTypeName, Ttl},
    errors::AppError,
};

use super::super::PostgresStorage;

// ---------------------------------------------------------------------------
// Enum-to-string helpers
// ---------------------------------------------------------------------------

pub(in crate::storage::postgres) fn record_owner_kind_value(
    kind: &RecordOwnerKind,
) -> &'static str {
    match kind {
        RecordOwnerKind::Host => "host",
        RecordOwnerKind::ForwardZone => "forward_zone",
        RecordOwnerKind::ForwardZoneDelegation => "forward_zone_delegation",
        RecordOwnerKind::ReverseZone => "reverse_zone",
        RecordOwnerKind::ReverseZoneDelegation => "reverse_zone_delegation",
        RecordOwnerKind::NameServer => "nameserver",
    }
}

pub(in crate::storage::postgres) fn record_cardinality_value(
    cardinality: &RecordCardinality,
) -> &'static str {
    match cardinality {
        RecordCardinality::Single => "single",
        RecordCardinality::Multiple => "multiple",
    }
}

pub(in crate::storage::postgres) fn record_type_storage_parts(
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

// ---------------------------------------------------------------------------
// Row types for raw SQL queries
// ---------------------------------------------------------------------------

#[derive(QueryableByName)]
pub(in crate::storage::postgres) struct ExistingRecordRow {
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
pub(in crate::storage::postgres) struct IntSentinelRow {
    #[diesel(sql_type = Integer)]
    pub value: i32,
}

// ---------------------------------------------------------------------------
// Builtin record types & export templates seeding
// ---------------------------------------------------------------------------

impl PostgresStorage {
    pub(in crate::storage::postgres) fn ensure_builtin_record_types(
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
                    record_types::dns_type.eq(command.dns_type().map(|v| v.as_i32())),
                    record_types::owner_kind.eq(&owner_kind),
                    record_types::cardinality.eq(&cardinality),
                    record_types::validation_schema.eq(&validation_schema),
                    record_types::rendering_schema.eq(&rendering_schema),
                    record_types::behavior_flags.eq(&behavior_flags),
                    record_types::built_in.eq(true),
                ))
                .on_conflict(record_types::name)
                .do_update()
                .set((
                    record_types::dns_type.eq(command.dns_type().map(|v| v.as_i32())),
                    record_types::owner_kind.eq(&owner_kind),
                    record_types::cardinality.eq(&cardinality),
                    record_types::validation_schema.eq(&validation_schema),
                    record_types::rendering_schema.eq(&rendering_schema),
                    record_types::behavior_flags.eq(&behavior_flags),
                    record_types::built_in.eq(true),
                    record_types::updated_at.eq(diesel::dsl::now),
                ))
                .execute(connection)?;
        }
        Self::ensure_builtin_export_templates(connection)?;
        Ok(())
    }

    pub(in crate::storage::postgres) fn ensure_builtin_export_templates(
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
                .do_update()
                .set((
                    export_templates::description.eq(command.description()),
                    export_templates::engine.eq(command.engine()),
                    export_templates::scope.eq(command.scope()),
                    export_templates::body.eq(command.body()),
                    export_templates::metadata.eq(command.metadata()),
                    export_templates::built_in.eq(true),
                    export_templates::updated_at.eq(diesel::dsl::now),
                ))
                .execute(connection)?;
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn render_record_data(
        type_name: &RecordTypeName,
        template: Option<&str>,
        data: &Value,
    ) -> Result<Option<String>, AppError> {
        render_rdata(type_name, template, data)
    }

    pub(in crate::storage::postgres) fn query_existing_owner_records(
        connection: &mut PgConnection,
        owner_name: &DnsName,
        zone_id: Option<Uuid>,
    ) -> Result<Vec<ExistingRecordSummary>, AppError> {
        let rows = sql_query(
            "SELECT rt.name::text AS type_name, rs.ttl, r.data, r.raw_rdata
             FROM records r
             JOIN rrsets rs ON rs.id = r.rrset_id
             JOIN record_types rt ON rt.id = rs.type_id
             WHERE rs.owner_name = $1
               AND rs.zone_id IS NOT DISTINCT FROM $2
             ORDER BY r.created_at",
        )
        .bind::<Text, _>(owner_name.as_str())
        .bind::<diesel::sql_types::Nullable<SqlUuid>, _>(zone_id)
        .load::<ExistingRecordRow>(connection)?;

        rows.into_iter()
            .map(ExistingRecordRow::into_summary)
            .collect()
    }

    pub(in crate::storage::postgres) fn query_existing_rrset_records(
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

    pub(in crate::storage::postgres) fn query_alias_owner_names(
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
                 WHERE (rt.name = 'CNAME' AND rs.owner_name = $1)
                    OR (rt.name = 'DNAME'
                        AND length($1) > length(rs.owner_name)
                        AND right($1, length(rs.owner_name) + 1) = '.' || rs.owner_name)
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

    pub(in crate::storage::postgres) fn validate_alias_graph_in_conn(
        connection: &mut PgConnection,
        type_name: &RecordTypeName,
        owner_name: &DnsName,
        zone_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        // Serialize graph transitions for the same owner. Database uniqueness
        // protects the RRset itself; this lock protects cross-owner edges.
        sql_query("SELECT pg_advisory_xact_lock(hashtext($1), hashtext($2))")
            .bind::<Text, _>(owner_name.as_str())
            .bind::<Text, _>(zone_id.map_or_else(|| "unassigned".to_string(), |id| id.to_string()))
            .execute(connection)?;

        let has_inbound_reference = type_name.as_str() == "CNAME"
            && sql_query(
                "SELECT 1 AS value
                 FROM records r
                 JOIN record_types rt ON rt.id = r.type_id
                 WHERE (rt.name = 'MX' AND r.data ->> 'exchange' = $1)
                    OR (rt.name = 'NS' AND r.data ->> 'nsdname' = $1)
                    OR (rt.name = 'PTR' AND r.data ->> 'ptrdname' = $1)
                    OR (rt.name = 'SRV' AND r.data ->> 'target' = $1)
                    OR (rt.name = 'NAPTR' AND r.data ->> 'replacement' = $1)
                 LIMIT 1",
            )
            .bind::<Text, _>(owner_name.as_str())
            .get_result::<IntSentinelRow>(connection)
            .optional()?
            .is_some();

        let has_descendant_data = type_name.as_str() == "DNAME"
            && sql_query(
                "SELECT 1 AS value FROM rrsets
                 WHERE length(owner_name) > length($1)
                   AND right(owner_name, length($1) + 1) = '.' || $1
                   AND zone_id IS NOT DISTINCT FROM $2
                 LIMIT 1",
            )
            .bind::<Text, _>(owner_name.as_str())
            .bind::<diesel::sql_types::Nullable<SqlUuid>, _>(zone_id)
            .get_result::<IntSentinelRow>(connection)
            .optional()?
            .is_some();

        let is_below_dname = sql_query(
            "SELECT 1 AS value
             FROM rrsets rs JOIN record_types rt ON rt.id = rs.type_id
             WHERE rt.name = 'DNAME'
               AND length($1) > length(rs.owner_name)
               AND right($1, length(rs.owner_name) + 1) = '.' || rs.owner_name
               AND rs.zone_id IS NOT DISTINCT FROM $2
             LIMIT 1",
        )
        .bind::<Text, _>(owner_name.as_str())
        .bind::<diesel::sql_types::Nullable<SqlUuid>, _>(zone_id)
        .get_result::<IntSentinelRow>(connection)
        .optional()?
        .is_some();

        crate::domain::resource_records::validate_alias_graph(
            type_name,
            owner_name,
            has_inbound_reference,
            has_descendant_data,
            is_below_dname,
        )
    }
}
