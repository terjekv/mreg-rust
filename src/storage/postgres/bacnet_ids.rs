use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Array, Integer, Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        bacnet::{BacnetIdAssignment, CreateBacnetIdAssignment},
        filters::BacnetIdFilter,
        pagination::{Page, PageRequest},
        types::{BacnetIdentifier, Hostname},
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, paginate_simple, run_dynamic_query},
    storage::{BacnetStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct BacnetIdRow {
    #[diesel(sql_type = Integer)]
    id: i32,
    #[diesel(sql_type = SqlUuid)]
    #[allow(dead_code)]
    host_id: Uuid,
    #[diesel(sql_type = Text)]
    host_name: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &BacnetIdFilter,
) -> Result<Page<BacnetIdAssignment>, AppError> {
    let (clauses, values) = filter.sql_conditions();
    let mut query = String::from(
        "SELECT b.id, b.host_id, h.name::text AS host_name,
                b.created_at, b.updated_at
         FROM bacnet_ids b
         JOIN hosts h ON h.id = b.host_id",
    );
    if !clauses.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&clauses.join(" AND "));
    }
    query.push_str(" ORDER BY b.id");
    let rows = run_dynamic_query::<BacnetIdRow>(connection, &query, &values)?;

    let all: Vec<BacnetIdAssignment> = rows
        .into_iter()
        .map(|row| {
            Ok(BacnetIdAssignment::restore(
                BacnetIdentifier::new(u32::try_from(row.id).map_err(|_| {
                    AppError::internal(format!("invalid bacnet_id in database: {}", row.id))
                })?)?,
                Hostname::new(row.host_name)?,
                row.created_at,
                row.updated_at,
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok(paginate_simple(all, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreateBacnetIdAssignment,
) -> Result<BacnetIdAssignment, AppError> {
    let host_id = PostgresStorage::resolve_host_id(connection, command.host_name())?;

    let row = sql_query(
        "INSERT INTO bacnet_ids (id, host_id)
         VALUES ($1, $2)
         RETURNING id, host_id, $3::text AS host_name,
                   created_at, updated_at",
    )
    .bind::<Integer, _>(command.bacnet_id().as_i32())
    .bind::<SqlUuid, _>(host_id)
    .bind::<Text, _>(command.host_name().as_str())
    .get_result::<BacnetIdRow>(connection)
    .map_err(map_unique("bacnet id is already assigned"))?;

    Ok(BacnetIdAssignment::restore(
        BacnetIdentifier::new(u32::try_from(row.id).map_err(|_| {
            AppError::internal(format!("invalid bacnet_id in database: {}", row.id))
        })?)?,
        Hostname::new(row.host_name)?,
        row.created_at,
        row.updated_at,
    ))
}

pub(super) fn get(
    connection: &mut PgConnection,
    bacnet_id: BacnetIdentifier,
) -> Result<BacnetIdAssignment, AppError> {
    let row = sql_query(
        "SELECT b.id, b.host_id, h.name::text AS host_name,
                b.created_at, b.updated_at
         FROM bacnet_ids b
         JOIN hosts h ON h.id = b.host_id
         WHERE b.id = $1",
    )
    .bind::<Integer, _>(bacnet_id.as_i32())
    .get_result::<BacnetIdRow>(connection)
    .optional()?
    .ok_or_else(|| {
        AppError::not_found(format!("bacnet id '{}' was not found", bacnet_id.as_u32()))
    })?;

    Ok(BacnetIdAssignment::restore(
        BacnetIdentifier::new(u32::try_from(row.id).map_err(|_| {
            AppError::internal(format!("invalid bacnet_id in database: {}", row.id))
        })?)?,
        Hostname::new(row.host_name)?,
        row.created_at,
        row.updated_at,
    ))
}

pub(super) fn list_for_hosts(
    connection: &mut PgConnection,
    hosts: &[Hostname],
) -> Result<Vec<BacnetIdAssignment>, AppError> {
    if hosts.is_empty() {
        return Ok(Vec::new());
    }
    let host_names = hosts
        .iter()
        .map(|host| host.as_str().to_string())
        .collect::<Vec<_>>();
    let rows = sql_query(
        "SELECT b.id, b.host_id, h.name::text AS host_name,
                b.created_at, b.updated_at
         FROM bacnet_ids b
         JOIN hosts h ON h.id = b.host_id
         WHERE h.name = ANY($1::text[])
         ORDER BY b.id",
    )
    .bind::<Array<Text>, _>(&host_names)
    .load::<BacnetIdRow>(connection)?;

    rows.into_iter()
        .map(|row| {
            Ok(BacnetIdAssignment::restore(
                BacnetIdentifier::new(u32::try_from(row.id).map_err(|_| {
                    AppError::internal(format!("invalid bacnet_id in database: {}", row.id))
                })?)?,
                Hostname::new(row.host_name)?,
                row.created_at,
                row.updated_at,
            ))
        })
        .collect()
}

pub(super) fn delete(
    connection: &mut PgConnection,
    bacnet_id: BacnetIdentifier,
) -> Result<(), AppError> {
    let deleted = sql_query("DELETE FROM bacnet_ids WHERE id = $1")
        .bind::<Integer, _>(bacnet_id.as_i32())
        .execute(connection)?;
    if deleted == 0 {
        return Err(AppError::not_found(format!(
            "bacnet id '{}' was not found",
            bacnet_id.as_u32()
        )));
    }
    Ok(())
}

#[async_trait]
impl BacnetStore for PostgresStorage {
    async fn list_bacnet_ids(
        &self,
        page: &PageRequest,
        filter: &BacnetIdFilter,
    ) -> Result<Page<BacnetIdAssignment>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_bacnet_id(
        &self,
        command: CreateBacnetIdAssignment,
    ) -> Result<BacnetIdAssignment, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_bacnet_id(
        &self,
        bacnet_id: BacnetIdentifier,
    ) -> Result<BacnetIdAssignment, AppError> {
        self.database
            .run(move |connection| get(connection, bacnet_id))
            .await
    }

    async fn list_bacnet_ids_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<BacnetIdAssignment>, AppError> {
        let hosts = hosts.to_vec();
        self.database
            .run(move |connection| list_for_hosts(connection, &hosts))
            .await
    }

    async fn delete_bacnet_id(&self, bacnet_id: BacnetIdentifier) -> Result<(), AppError> {
        self.database
            .run(move |connection| delete(connection, bacnet_id))
            .await
    }
}
