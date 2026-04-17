use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        filters::PtrOverrideFilter,
        pagination::{Page, PageRequest},
        ptr_override::{CreatePtrOverride, PtrOverride},
        types::{DnsName, Hostname, IpAddressValue},
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, vec_to_page},
    storage::{PtrOverrideStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct PtrOverrideRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    host_name: String,
    #[diesel(sql_type = Text)]
    address: String,
    #[diesel(sql_type = Nullable<Text>)]
    target_name: Option<String>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &PtrOverrideFilter,
) -> Result<Page<PtrOverride>, AppError> {
    let rows = sql_query(
        "SELECT p.id, h.name::text AS host_name,
                host(p.address) AS address,
                p.target_name::text AS target_name,
                p.created_at, p.updated_at
         FROM ptr_overrides p
         JOIN hosts h ON h.id = p.host_id
         ORDER BY p.address",
    )
    .load::<PtrOverrideRow>(connection)?;

    let all: Vec<PtrOverride> = rows
        .into_iter()
        .map(|row| {
            Ok(PtrOverride::restore(
                row.id,
                Hostname::new(row.host_name)?,
                IpAddressValue::new(row.address)?,
                row.target_name.map(DnsName::new).transpose()?,
                row.created_at,
                row.updated_at,
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let items: Vec<PtrOverride> = all.into_iter().filter(|ptr| filter.matches(ptr)).collect();

    Ok(vec_to_page(items, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreatePtrOverride,
) -> Result<PtrOverride, AppError> {
    let host_id = PostgresStorage::resolve_host_id(connection, command.host_name())?;

    let row = sql_query(
        "INSERT INTO ptr_overrides (host_id, address, target_name)
         VALUES ($1, $2::inet, $3)
         RETURNING id, $4::text AS host_name,
                   host(address) AS address,
                   target_name::text AS target_name,
                   created_at, updated_at",
    )
    .bind::<SqlUuid, _>(host_id)
    .bind::<Text, _>(command.address().as_str())
    .bind::<Nullable<Text>, _>(command.target_name().map(|name| name.as_str().to_string()))
    .bind::<Text, _>(command.host_name().as_str())
    .get_result::<PtrOverrideRow>(connection)
    .map_err(map_unique("ptr override already exists for this address"))?;

    Ok(PtrOverride::restore(
        row.id,
        Hostname::new(row.host_name)?,
        IpAddressValue::new(row.address)?,
        row.target_name.map(DnsName::new).transpose()?,
        row.created_at,
        row.updated_at,
    ))
}

pub(super) fn get_by_address(
    connection: &mut PgConnection,
    addr: &str,
) -> Result<PtrOverride, AppError> {
    let row = sql_query(
        "SELECT p.id, h.name::text AS host_name,
                host(p.address) AS address,
                p.target_name::text AS target_name,
                p.created_at, p.updated_at
         FROM ptr_overrides p
         JOIN hosts h ON h.id = p.host_id
         WHERE p.address = $1::inet",
    )
    .bind::<Text, _>(addr)
    .get_result::<PtrOverrideRow>(connection)
    .optional()?
    .ok_or_else(|| AppError::not_found(format!("ptr override for '{}' was not found", addr)))?;

    Ok(PtrOverride::restore(
        row.id,
        Hostname::new(row.host_name)?,
        IpAddressValue::new(row.address)?,
        row.target_name.map(DnsName::new).transpose()?,
        row.created_at,
        row.updated_at,
    ))
}

pub(super) fn delete(connection: &mut PgConnection, addr: &str) -> Result<(), AppError> {
    let deleted = sql_query("DELETE FROM ptr_overrides WHERE address = $1::inet")
        .bind::<Text, _>(addr)
        .execute(connection)?;
    if deleted == 0 {
        return Err(AppError::not_found(format!(
            "ptr override for '{}' was not found",
            addr
        )));
    }
    Ok(())
}

#[async_trait]
impl PtrOverrideStore for PostgresStorage {
    async fn list_ptr_overrides(
        &self,
        page: &PageRequest,
        filter: &PtrOverrideFilter,
    ) -> Result<Page<PtrOverride>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_ptr_override(
        &self,
        command: CreatePtrOverride,
    ) -> Result<PtrOverride, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_ptr_override_by_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<PtrOverride, AppError> {
        let addr = address.as_str();
        self.database
            .run(move |connection| get_by_address(connection, &addr))
            .await
    }

    async fn delete_ptr_override(&self, address: &IpAddressValue) -> Result<(), AppError> {
        let addr = address.as_str();
        self.database
            .run(move |connection| delete(connection, &addr))
            .await
    }
}
