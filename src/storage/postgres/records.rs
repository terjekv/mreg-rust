use std::collections::BTreeMap;

use async_trait::async_trait;
use diesel::{
    Connection, ExpressionMethods, JoinOnDsl, OptionalExtension, PgConnection, QueryDsl,
    RunQueryDsl, SelectableHelper, insert_into, sql_query,
    sql_types::{Bytea, Integer, Nullable, Text, Uuid as SqlUuid},
};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    db::{
        models::{RecordRow, RecordTypeRow, RrsetRow},
        schema::record_types,
    },
    domain::{
        filters::RecordFilter,
        pagination::{Page, PageRequest},
        resource_records::{
            CreateRecordInstance, CreateRecordTypeDefinition, RecordInstance, RecordRrset,
            RecordTypeDefinition, UpdateRecord, ValidatedRecordContent, alias_target_names,
            validate_record_relationships,
        },
        types::{DnsName, RecordTypeName},
    },
    errors::AppError,
    storage::RecordStore,
};

use super::PostgresStorage;
use super::helpers::{
    IntSentinelRow, map_unique, record_type_storage_parts, run_dynamic_query, vec_to_page,
};

impl PostgresStorage {
    pub(super) fn query_record_type_by_name(
        connection: &mut PgConnection,
        name: &RecordTypeName,
    ) -> Result<RecordTypeDefinition, AppError> {
        Self::ensure_builtin_record_types(connection)?;
        record_types::table
            .filter(record_types::name.eq(name.as_str()))
            .select(RecordTypeRow::as_select())
            .first::<RecordTypeRow>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!("record type '{}' was not found", name.as_str()))
            })?
            .into_domain()
    }

    pub(super) fn query_record_types(
        connection: &mut PgConnection,
    ) -> Result<Vec<RecordTypeDefinition>, AppError> {
        Self::ensure_builtin_record_types(connection)?;
        let rows = record_types::table
            .select(RecordTypeRow::as_select())
            .order(record_types::name.asc())
            .load::<RecordTypeRow>(connection)?;
        rows.into_iter().map(RecordTypeRow::into_domain).collect()
    }

    pub(super) fn query_rrsets(
        connection: &mut PgConnection,
    ) -> Result<Vec<RecordRrset>, AppError> {
        Self::ensure_builtin_record_types(connection)?;
        let rows = sql_query(
            "SELECT rs.id, rs.type_id, rt.name::text AS type_name, rs.dns_class,
                    rs.owner_name::text AS owner_name, rs.anchor_kind, rs.anchor_id,
                    rs.anchor_name::text AS anchor_name, rs.zone_id, rs.ttl,
                    rs.created_at, rs.updated_at
             FROM rrsets rs
             JOIN record_types rt ON rt.id = rs.type_id
             ORDER BY rs.owner_name, rt.name",
        )
        .load::<RrsetRow>(connection)?;
        rows.into_iter().map(RrsetRow::into_domain).collect()
    }

    pub(super) fn query_records(
        connection: &mut PgConnection,
    ) -> Result<Vec<RecordInstance>, AppError> {
        Self::ensure_builtin_record_types(connection)?;
        let rows = sql_query(
            "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind,
                    rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl,
                    r.data, r.raw_rdata, r.rendered,
                    r.created_at, r.updated_at
             FROM records r
             JOIN rrsets rs ON rs.id = r.rrset_id
             JOIN record_types rt ON rt.id = rs.type_id
             ORDER BY r.created_at DESC",
        )
        .load::<RecordRow>(connection)?;
        rows.into_iter().map(RecordRow::into_domain).collect()
    }

    pub(super) fn query_records_for_zone(
        connection: &mut PgConnection,
        zone_id: Uuid,
    ) -> Result<Vec<RecordInstance>, AppError> {
        Self::ensure_builtin_record_types(connection)?;
        let rows = sql_query(
            "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind,
                    rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl,
                    r.data, r.raw_rdata, r.rendered,
                    r.created_at, r.updated_at
             FROM records r
             JOIN rrsets rs ON rs.id = r.rrset_id
             JOIN record_types rt ON rt.id = rs.type_id
             WHERE rs.zone_id = $1
             ORDER BY r.created_at DESC",
        )
        .bind::<diesel::sql_types::Uuid, _>(zone_id)
        .load::<RecordRow>(connection)?;
        rows.into_iter().map(RecordRow::into_domain).collect()
    }

    pub(super) fn query_rrset_by_type_and_owner(
        connection: &mut PgConnection,
        type_id: Uuid,
        owner_name: &DnsName,
    ) -> Result<Option<RecordRrset>, AppError> {
        sql_query(
            "SELECT rs.id, rs.type_id, rt.name::text AS type_name, rs.dns_class,
                    rs.owner_name::text AS owner_name, rs.anchor_kind, rs.anchor_id,
                    rs.anchor_name::text AS anchor_name, rs.zone_id, rs.ttl,
                    rs.created_at, rs.updated_at
             FROM rrsets rs
             JOIN record_types rt ON rt.id = rs.type_id
             WHERE rs.type_id = $1 AND rs.owner_name = $2 AND rs.dns_class = 'IN'
             LIMIT 1",
        )
        .bind::<SqlUuid, _>(type_id)
        .bind::<Text, _>(owner_name.as_str())
        .get_result::<RrsetRow>(connection)
        .optional()?
        .map(RrsetRow::into_domain)
        .transpose()
    }

    pub(super) fn insert_rrset(
        connection: &mut PgConnection,
        record_type: &RecordTypeDefinition,
        command: &CreateRecordInstance,
        anchor_id: Option<Uuid>,
        zone_id: Option<Uuid>,
    ) -> Result<RecordRrset, AppError> {
        use super::helpers::record_owner_kind_value;

        sql_query(
            "INSERT INTO rrsets
                (type_id, dns_class, owner_name, anchor_kind, anchor_id, anchor_name, zone_id, ttl)
             VALUES ($1, 'IN', $2, $3, $4, $5, $6, $7)
             RETURNING id, type_id, $8::text AS type_name, dns_class, owner_name::text AS owner_name,
                       anchor_kind, anchor_id, anchor_name::text AS anchor_name, zone_id, ttl,
                       created_at, updated_at",
        )
        .bind::<SqlUuid, _>(record_type.id())
        .bind::<Text, _>(command.owner_name().as_str())
        .bind::<Nullable<Text>, _>(command.owner_kind().map(record_owner_kind_value))
        .bind::<Nullable<SqlUuid>, _>(anchor_id)
        .bind::<Nullable<Text>, _>(command.anchor_name())
        .bind::<Nullable<SqlUuid>, _>(zone_id)
        .bind::<Nullable<Integer>, _>(command.ttl().map(|ttl| ttl.as_i32()))
        .bind::<Text, _>(record_type.name().as_str())
        .get_result::<RrsetRow>(connection)?
        .into_domain()
    }

    pub(super) fn insert_record(
        connection: &mut PgConnection,
        rrset: &RecordRrset,
        rendered: Option<String>,
        content: &ValidatedRecordContent,
    ) -> Result<RecordInstance, AppError> {
        use super::helpers::record_owner_kind_value;

        let (data, raw_rdata) = match content {
            ValidatedRecordContent::Structured(value) => (value.clone(), None),
            ValidatedRecordContent::RawRdata(raw) => (Value::Null, Some(raw.wire_bytes().to_vec())),
        };

        sql_query(
            "INSERT INTO records
                (type_id, owner_kind, owner_id, owner_name, zone_id, ttl, data, rendered, rrset_id, raw_rdata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id, rrset_id, $11::uuid AS type_id, $12::text AS type_name, $13::text AS anchor_kind,
                       $14::uuid AS anchor_id, $15::text AS owner_name, $16::uuid AS zone_id, $17::int AS ttl,
                       data, raw_rdata, rendered, created_at, updated_at",
        )
        .bind::<SqlUuid, _>(rrset.type_id())
        .bind::<Nullable<Text>, _>(rrset.anchor_kind().map(record_owner_kind_value))
        .bind::<Nullable<SqlUuid>, _>(rrset.anchor_id())
        .bind::<Text, _>(rrset.owner_name().as_str())
        .bind::<Nullable<SqlUuid>, _>(rrset.zone_id())
        .bind::<Nullable<Integer>, _>(rrset.ttl().map(|ttl| ttl.as_i32()))
        .bind::<diesel::sql_types::Jsonb, _>(data)
        .bind::<Nullable<Text>, _>(rendered)
        .bind::<SqlUuid, _>(rrset.id())
        .bind::<Nullable<Bytea>, _>(raw_rdata)
        .bind::<SqlUuid, _>(rrset.type_id())
        .bind::<Text, _>(rrset.type_name().as_str())
        .bind::<Nullable<Text>, _>(rrset.anchor_kind().map(record_owner_kind_value))
        .bind::<Nullable<SqlUuid>, _>(rrset.anchor_id())
        .bind::<Text, _>(rrset.owner_name().as_str())
        .bind::<Nullable<SqlUuid>, _>(rrset.zone_id())
        .bind::<Nullable<Integer>, _>(rrset.ttl().map(|ttl| ttl.as_i32()))
        .get_result::<RecordRow>(connection)?
        .into_domain()
    }
}

#[async_trait]
impl RecordStore for PostgresStorage {
    async fn list_record_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_record_types(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_rrsets(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database.run(move |c| {
            Self::ensure_builtin_record_types(c)?;

            let base = "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind, \
                        rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl, \
                        r.data, r.raw_rdata, r.rendered, \
                        r.created_at, r.updated_at \
                        FROM records r \
                        JOIN rrsets rs ON rs.id = r.rrset_id \
                        JOIN record_types rt ON rt.id = rs.type_id";

            let (clauses, values) = filter.sql_conditions();
            let where_str = if clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", clauses.join(" AND "))
            };
            let query_str = format!("{base}{where_str} ORDER BY r.created_at DESC");

            let rows = run_dynamic_query::<RecordRow>(c, &query_str, &values)?;
            let items: Vec<RecordInstance> = rows.into_iter()
                .map(RecordRow::into_domain)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(vec_to_page(items, &page))
        }).await
    }

    async fn create_record_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError> {
        let name = command.name().as_str().to_string();
        let dns_type = command.dns_type();
        let (owner_kind, cardinality, validation_schema, rendering_schema, behavior_flags) =
            record_type_storage_parts(command.schema());
        let built_in = command.built_in();
        self.database
            .run(move |connection| {
                Self::ensure_builtin_record_types(connection)?;
                insert_into(record_types::table)
                    .values((
                        record_types::name.eq(&name),
                        record_types::dns_type.eq(dns_type),
                        record_types::owner_kind.eq(&owner_kind),
                        record_types::cardinality.eq(&cardinality),
                        record_types::validation_schema.eq(&validation_schema),
                        record_types::rendering_schema.eq(&rendering_schema),
                        record_types::behavior_flags.eq(&behavior_flags),
                        record_types::built_in.eq(built_in),
                    ))
                    .returning(RecordTypeRow::as_returning())
                    .get_result(connection)
                    .map_err(map_unique("record type already exists"))?
                    .into_domain()
            })
            .await
    }

    async fn get_record(&self, record_id: Uuid) -> Result<RecordInstance, AppError> {
        self.database
            .run(move |connection| {
                sql_query(
                    "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind,
                            rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl,
                            r.data, r.raw_rdata, r.rendered,
                            r.created_at, r.updated_at
                     FROM records r
                     JOIN rrsets rs ON rs.id = r.rrset_id
                     JOIN record_types rt ON rt.id = rs.type_id
                     WHERE r.id = $1",
                )
                .bind::<SqlUuid, _>(record_id)
                .get_result::<RecordRow>(connection)
                .optional()?
                .ok_or_else(|| AppError::not_found("record not found"))?
                .into_domain()
            })
            .await
    }

    async fn get_rrset(&self, rrset_id: Uuid) -> Result<RecordRrset, AppError> {
        self.database
            .run(move |connection| {
                sql_query(
                    "SELECT rs.id, rs.type_id, rt.name::text AS type_name, rs.dns_class,
                            rs.owner_name::text AS owner_name, rs.anchor_kind, rs.anchor_id,
                            rs.anchor_name::text AS anchor_name, rs.zone_id, rs.ttl,
                            rs.created_at, rs.updated_at
                     FROM rrsets rs
                     JOIN record_types rt ON rt.id = rs.type_id
                     WHERE rs.id = $1",
                )
                .bind::<SqlUuid, _>(rrset_id)
                .get_result::<RrsetRow>(connection)
                .optional()?
                .ok_or_else(|| AppError::not_found("rrset not found"))?
                .into_domain()
            })
            .await
    }

    async fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<RecordInstance, AppError, _>(|connection| {
                    let record_type =
                        Self::query_record_type_by_name(connection, command.type_name())?;
                    let (anchor_id, _anchor_name, zone_id) = Self::resolve_record_owner(
                        connection,
                        command.owner_kind(),
                        command.anchor_name(),
                        command.owner_name(),
                    )?;
                    let validated = record_type.validate_record_input(
                        command.owner_name(),
                        command.data(),
                        command.raw_rdata(),
                    )?;
                    let same_owner_records =
                        Self::query_existing_owner_records(connection, command.owner_name())?;
                    let existing_rrset = Self::query_rrset_by_type_and_owner(
                        connection,
                        record_type.id(),
                        command.owner_name(),
                    )?;
                    let same_rrset_records = if let Some(rrset) = &existing_rrset {
                        Self::query_existing_rrset_records(connection, rrset.id())?
                    } else {
                        Vec::new()
                    };
                    let alias_lookup = match &validated {
                        ValidatedRecordContent::Structured(normalized) => {
                            Self::query_alias_owner_names(
                                connection,
                                &alias_target_names(normalized, record_type.name()),
                            )?
                        }
                        ValidatedRecordContent::RawRdata(_) => BTreeMap::new(),
                    };
                    let alias_owner_names = alias_lookup
                        .into_iter()
                        .filter_map(|(name, is_alias)| is_alias.then_some(name))
                        .collect();
                    validate_record_relationships(
                        &record_type,
                        command.ttl(),
                        &validated,
                        &same_owner_records,
                        &same_rrset_records,
                        &alias_owner_names,
                    )?;
                    let rrset = if let Some(rrset) = existing_rrset {
                        rrset
                    } else {
                        Self::insert_rrset(connection, &record_type, &command, anchor_id, zone_id)?
                    };
                    let rendered =
                        if let ValidatedRecordContent::Structured(normalized) = &validated {
                            Self::render_record_data(
                                record_type.schema().render_template(),
                                normalized,
                            )?
                        } else {
                            None
                        };
                    let record = Self::insert_record(connection, &rrset, rendered, &validated)?;
                    // Cascade: bump zone serial
                    if let Some(zone_id) = record.zone_id() {
                        Self::bump_zone_serial_tx(connection, zone_id);
                    }
                    Ok(record)
                })
            })
            .await
    }

    async fn update_record(
        &self,
        record_id: Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<RecordInstance, AppError, _>(|connection| {
                    // Fetch existing record
                    let existing = sql_query(
                        "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind,
                                rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl,
                                r.data, r.raw_rdata, r.rendered,
                                r.created_at, r.updated_at
                         FROM records r
                         JOIN rrsets rs ON rs.id = r.rrset_id
                         JOIN record_types rt ON rt.id = rs.type_id
                         WHERE r.id = $1",
                    )
                    .bind::<SqlUuid, _>(record_id)
                    .get_result::<RecordRow>(connection)
                    .optional()?
                    .ok_or_else(|| AppError::not_found("record not found"))?
                    .into_domain()?;

                    let record_type = Self::query_record_type_by_name(connection, existing.type_name())?;
                    let owner_name = DnsName::new(existing.owner_name())?;

                    let new_ttl = match command.ttl() {
                        Some(ttl_opt) => ttl_opt,
                        None => existing.ttl(),
                    };

                    let data_changed = command.data().is_some() || command.raw_rdata().is_some();

                    let (new_data, new_raw_rdata, new_rendered) = if data_changed {
                        let validated = record_type.validate_record_input(
                            &owner_name,
                            command.data(),
                            command.raw_rdata(),
                        )?;

                        let same_owner_records = {
                            let mut all = Self::query_existing_owner_records(connection, &owner_name)?;
                            // Exclude self: remove the record that matches our existing data
                            all.retain(|r| {
                                !(r.type_name() == existing.type_name()
                                    && r.data() == existing.data()
                                    && r.raw_rdata() == existing.raw_rdata())
                            });
                            all
                        };

                        let same_rrset_records = {
                            let mut all = Self::query_existing_rrset_records(connection, existing.rrset_id())?;
                            all.retain(|r| {
                                !(r.type_name() == existing.type_name()
                                    && r.data() == existing.data()
                                    && r.raw_rdata() == existing.raw_rdata())
                            });
                            all
                        };

                        let alias_lookup = match &validated {
                            ValidatedRecordContent::Structured(normalized) => {
                                Self::query_alias_owner_names(
                                    connection,
                                    &alias_target_names(normalized, record_type.name()),
                                )?
                            }
                            ValidatedRecordContent::RawRdata(_) => BTreeMap::new(),
                        };
                        let alias_owner_names = alias_lookup
                            .into_iter()
                            .filter_map(|(name, is_alias)| is_alias.then_some(name))
                            .collect();

                        validate_record_relationships(
                            &record_type,
                            new_ttl,
                            &validated,
                            &same_owner_records,
                            &same_rrset_records,
                            &alias_owner_names,
                        )?;

                        let rendered = if let ValidatedRecordContent::Structured(normalized) = &validated {
                            Self::render_record_data(
                                record_type.schema().render_template(),
                                normalized,
                            )?
                        } else {
                            None
                        };

                        match validated {
                            ValidatedRecordContent::Structured(data) => (data, None::<Vec<u8>>, rendered),
                            ValidatedRecordContent::RawRdata(raw) => (Value::Null, Some(raw.wire_bytes().to_vec()), None),
                        }
                    } else {
                        (
                            existing.data().clone(),
                            existing.raw_rdata().map(|r| r.wire_bytes().to_vec()),
                            existing.rendered().map(|s| s.to_string()),
                        )
                    };

                    {
                        use crate::db::schema::records;
                        diesel::update(records::table.filter(records::id.eq(record_id)))
                            .set((
                                records::ttl.eq(new_ttl.map(|t| t.as_i32())),
                                records::data.eq(&new_data),
                                records::raw_rdata.eq(&new_raw_rdata),
                                records::rendered.eq(&new_rendered),
                                records::updated_at.eq(diesel::dsl::now),
                            ))
                            .execute(connection)?;
                    }

                    // Re-fetch the updated record
                    let record = sql_query(
                        "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind,
                                rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl,
                                r.data, r.raw_rdata, r.rendered,
                                r.created_at, r.updated_at
                         FROM records r
                         JOIN rrsets rs ON rs.id = r.rrset_id
                         JOIN record_types rt ON rt.id = rs.type_id
                         WHERE r.id = $1",
                    )
                    .bind::<SqlUuid, _>(record_id)
                    .get_result::<RecordRow>(connection)?
                    .into_domain()?;

                    // Cascade: bump zone serial
                    if let Some(zone_id) = record.zone_id() {
                        Self::bump_zone_serial_tx(connection, zone_id);
                    }
                    Ok(record)
                })
            })
            .await
    }

    async fn delete_record(&self, record_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<(), AppError, _>(|connection| {
                    use crate::db::schema::{records, rrsets};

                    // Fetch zone_id and rrset_id before deleting
                    let record_info = records::table
                        .inner_join(rrsets::table.on(rrsets::id.eq(records::rrset_id)))
                        .filter(records::id.eq(record_id))
                        .select((records::rrset_id, rrsets::zone_id))
                        .first::<(Uuid, Option<Uuid>)>(connection)
                        .optional()?;

                    let (rrset_id, zone_id) =
                        record_info.ok_or_else(|| AppError::not_found("record not found"))?;

                    diesel::delete(records::table.filter(records::id.eq(record_id)))
                        .execute(connection)?;

                    sql_query(
                        "DELETE FROM rrsets WHERE id = $1
                         AND NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = $1)",
                    )
                    .bind::<SqlUuid, _>(rrset_id)
                    .execute(connection)?;

                    // Cascade: bump zone serial
                    if let Some(zone_id) = zone_id {
                        Self::bump_zone_serial_tx(connection, zone_id);
                    }

                    Ok(())
                })
            })
            .await
    }

    async fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError> {
        let name_str = name.as_str().to_string();
        self.database
            .run(move |connection| {
                connection.transaction::<(), AppError, _>(|connection| {
                    // Check if built-in
                    let built_in_val = record_types::table
                        .filter(record_types::name.eq(&name_str))
                        .select(record_types::built_in)
                        .first::<bool>(connection)
                        .optional()?
                        .ok_or_else(|| AppError::not_found("record type not found"))?;

                    if built_in_val {
                        return Err(AppError::conflict("cannot delete built-in record type"));
                    }

                    // Check if records exist (uses JOIN so stays as sql_query)
                    let has_records = sql_query(
                        "SELECT 1 AS value FROM records r
                         JOIN rrsets rs ON rs.id = r.rrset_id
                         JOIN record_types rt ON rt.id = rs.type_id
                         WHERE rt.name = $1
                         LIMIT 1",
                    )
                    .bind::<Text, _>(&name_str)
                    .get_result::<IntSentinelRow>(connection)
                    .optional()?
                    .is_some();

                    if has_records {
                        return Err(AppError::conflict(
                            "cannot delete record type with existing records",
                        ));
                    }

                    let deleted = diesel::delete(
                        record_types::table.filter(record_types::name.eq(&name_str)),
                    )
                    .execute(connection)?;

                    if deleted == 0 {
                        return Err(AppError::not_found("record type not found"));
                    }

                    Ok(())
                })
            })
            .await
    }

    async fn delete_rrset(&self, rrset_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<(), AppError, _>(|connection| {
                    use crate::db::schema::rrsets;

                    // Fetch zone_id before deleting, so we can bump serial
                    let zone_id = rrsets::table
                        .filter(rrsets::id.eq(rrset_id))
                        .select(rrsets::zone_id)
                        .first::<Option<Uuid>>(connection)
                        .optional()?
                        .flatten();

                    let deleted = diesel::delete(rrsets::table.filter(rrsets::id.eq(rrset_id)))
                        .execute(connection)?;
                    if deleted == 0 {
                        return Err(AppError::not_found("rrset not found"));
                    }

                    // Cascade: bump zone serial
                    if let Some(zone_id) = zone_id {
                        Self::bump_zone_serial_tx(connection, zone_id);
                    }

                    Ok(())
                })
            })
            .await
    }

    async fn find_records_by_owner(&self, owner_id: Uuid) -> Result<Vec<RecordInstance>, AppError> {
        self.database
            .run(move |connection| {
                let rows = sql_query(
                    "SELECT r.id, r.rrset_id, rs.type_id, rt.name::text AS type_name, rs.anchor_kind,
                            rs.anchor_id, rs.owner_name::text AS owner_name, rs.zone_id, rs.ttl,
                            r.data, r.raw_rdata, r.rendered,
                            r.created_at, r.updated_at
                     FROM records r
                     JOIN rrsets rs ON rs.id = r.rrset_id
                     JOIN record_types rt ON rt.id = rs.type_id
                     WHERE r.owner_id = $1",
                )
                .bind::<SqlUuid, _>(owner_id)
                .load::<RecordRow>(connection)?;
                rows.into_iter().map(RecordRow::into_domain).collect()
            })
            .await
    }

    async fn delete_records_by_owner(&self, owner_id: Uuid) -> Result<u64, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<u64, AppError, _>(|connection| {
                    use crate::db::schema::records;

                    let deleted = diesel::delete(
                        records::table.filter(records::owner_id.eq(owner_id)),
                    )
                    .execute(connection)?;
                    sql_query(
                        "DELETE FROM rrsets WHERE NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = rrsets.id)",
                    )
                    .execute(connection)?;
                    Ok(deleted as u64)
                })
            })
            .await
    }

    async fn delete_records_by_owner_name_and_type(
        &self,
        owner_name: &DnsName,
        type_name: &RecordTypeName,
    ) -> Result<u64, AppError> {
        let owner_name_str = owner_name.as_str().to_string();
        let type_name_str = type_name.as_str().to_string();
        self.database
            .run(move |connection| {
                connection.transaction::<u64, AppError, _>(|connection| {
                    let deleted = sql_query(
                        "DELETE FROM records r
                         USING rrsets rs, record_types rt
                         WHERE r.rrset_id = rs.id AND rs.type_id = rt.id
                         AND rs.owner_name = $1 AND rt.name = $2",
                    )
                    .bind::<Text, _>(&owner_name_str)
                    .bind::<Text, _>(&type_name_str)
                    .execute(connection)?;
                    sql_query(
                        "DELETE FROM rrsets WHERE NOT EXISTS (SELECT 1 FROM records WHERE rrset_id = rrsets.id)",
                    )
                    .execute(connection)?;
                    Ok(deleted as u64)
                })
            })
            .await
    }

    async fn rename_record_owner(
        &self,
        owner_id: Uuid,
        new_name: &DnsName,
    ) -> Result<u64, AppError> {
        let new_name_str = new_name.as_str().to_string();
        self.database
            .run(move |connection| {
                connection.transaction::<u64, AppError, _>(|connection| {
                    use crate::db::schema::{records, rrsets};

                    diesel::update(rrsets::table.filter(rrsets::anchor_id.eq(owner_id)))
                        .set((
                            rrsets::owner_name.eq(&new_name_str),
                            rrsets::anchor_name.eq(&new_name_str),
                        ))
                        .execute(connection)?;
                    let updated =
                        diesel::update(records::table.filter(records::owner_id.eq(owner_id)))
                            .set(records::owner_name.eq(&new_name_str))
                            .execute(connection)?;
                    Ok(updated as u64)
                })
            })
            .await
    }
}
