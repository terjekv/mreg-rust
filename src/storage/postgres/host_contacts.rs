use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    Connection, OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Array, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    domain::{
        filters::HostContactFilter,
        host_contact::{CreateHostContact, HostContact},
        pagination::{Page, PageRequest},
        types::{EmailAddressValue, Hostname},
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, run_dynamic_query, vec_to_page},
    storage::{HostContactStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct HostContactRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    email: String,
    #[diesel(sql_type = Nullable<Text>)]
    display_name: Option<String>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

#[derive(QueryableByName)]
struct JunctionHostNameRow {
    #[diesel(sql_type = Text)]
    host_name: String,
}

#[derive(QueryableByName)]
struct ContactHostAssociationRow {
    #[diesel(sql_type = SqlUuid)]
    contact_id: Uuid,
    #[diesel(sql_type = Text)]
    host_name: String,
}

fn load_contact_hosts(
    connection: &mut PgConnection,
    contact_id: Uuid,
) -> Result<Vec<Hostname>, AppError> {
    let rows = sql_query(
        "SELECT h.name::text AS host_name
         FROM host_contacts_hosts hch
         JOIN hosts h ON h.id = hch.host_id
         WHERE hch.contact_id = $1
         ORDER BY h.name",
    )
    .bind::<SqlUuid, _>(contact_id)
    .load::<JunctionHostNameRow>(connection)?;

    rows.into_iter()
        .map(|row| Hostname::new(row.host_name))
        .collect()
}

/// Load all host associations for a batch of contact IDs in a single query.
fn load_contact_hosts_batch(
    connection: &mut PgConnection,
    contact_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<Hostname>>, AppError> {
    if contact_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sql_query(
        "SELECT hch.contact_id, h.name::text AS host_name
         FROM host_contacts_hosts hch
         JOIN hosts h ON h.id = hch.host_id
         WHERE hch.contact_id = ANY($1::uuid[])
         ORDER BY hch.contact_id, h.name",
    )
    .bind::<Array<SqlUuid>, _>(contact_ids)
    .load::<ContactHostAssociationRow>(connection)?;

    let mut map: HashMap<Uuid, Vec<Hostname>> = HashMap::new();
    for row in rows {
        let hostname = Hostname::new(row.host_name)?;
        map.entry(row.contact_id).or_default().push(hostname);
    }
    Ok(map)
}

fn build_host_contact(
    connection: &mut PgConnection,
    row: HostContactRow,
) -> Result<HostContact, AppError> {
    let hosts = load_contact_hosts(connection, row.id)?;
    HostContact::restore(
        row.id,
        EmailAddressValue::new(row.email)?,
        row.display_name,
        hosts,
        row.created_at,
        row.updated_at,
    )
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &HostContactFilter,
) -> Result<Page<HostContact>, AppError> {
    let base = "SELECT hc.id, hc.email::text AS email, hc.display_name, \
                hc.created_at, hc.updated_at \
                FROM host_contacts hc";

    let (clauses, values) = filter.sql_conditions();
    let where_str = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let query_str = format!("{base}{where_str} ORDER BY hc.email");

    let rows = run_dynamic_query::<HostContactRow>(connection, &query_str, &values)?;

    // Batch-load all host associations in a single query instead of N+1.
    let contact_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut hosts_by_contact = load_contact_hosts_batch(connection, &contact_ids)?;

    let all: Vec<HostContact> = rows
        .into_iter()
        .map(|row| {
            let hosts = hosts_by_contact.remove(&row.id).unwrap_or_default();
            HostContact::restore(
                row.id,
                EmailAddressValue::new(row.email)?,
                row.display_name,
                hosts,
                row.created_at,
                row.updated_at,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Apply special filters (host, search) in Rust
    let items: Vec<HostContact> = all
        .into_iter()
        .filter(|contact| filter.matches(contact))
        .collect();

    Ok(vec_to_page(items, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreateHostContact,
) -> Result<HostContact, AppError> {
    connection.transaction::<HostContact, AppError, _>(|connection| {
        let row = sql_query(
            "INSERT INTO host_contacts (email, display_name)
             VALUES ($1, $2)
             RETURNING id, email::text AS email, display_name,
                       created_at, updated_at",
        )
        .bind::<Text, _>(command.email().as_str())
        .bind::<Nullable<Text>, _>(command.display_name())
        .get_result::<HostContactRow>(connection)
        .map_err(map_unique("host contact already exists"))?;

        let contact_id = row.id;

        let hosts = command.hosts();
        if !hosts.is_empty() {
            let host_ids = PostgresStorage::resolve_host_ids(connection, hosts)?;
            for host_name in hosts {
                let host_id = host_ids[host_name];
                sql_query(
                    "INSERT INTO host_contacts_hosts (host_id, contact_id)
                     VALUES ($1, $2)",
                )
                .bind::<SqlUuid, _>(host_id)
                .bind::<SqlUuid, _>(contact_id)
                .execute(connection)?;
            }
        }

        build_host_contact(connection, row)
    })
}

pub(super) fn get_by_email(
    connection: &mut PgConnection,
    email: &str,
) -> Result<HostContact, AppError> {
    let row = sql_query(
        "SELECT id, email::text AS email, display_name,
                created_at, updated_at
         FROM host_contacts
         WHERE email = $1",
    )
    .bind::<Text, _>(email)
    .get_result::<HostContactRow>(connection)
    .optional()?
    .ok_or_else(|| AppError::not_found(format!("host contact '{}' was not found", email)))?;

    build_host_contact(connection, row)
}

pub(super) fn delete(connection: &mut PgConnection, email: &str) -> Result<(), AppError> {
    connection.transaction::<(), AppError, _>(|connection| {
        let contact_id = sql_query("SELECT id FROM host_contacts WHERE email = $1")
            .bind::<Text, _>(email)
            .get_result::<crate::db::models::UuidRow>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found(format!("host contact '{}' was not found", email)))?
            .id();

        sql_query("DELETE FROM host_contacts_hosts WHERE contact_id = $1")
            .bind::<SqlUuid, _>(contact_id)
            .execute(connection)?;

        sql_query("DELETE FROM host_contacts WHERE id = $1")
            .bind::<SqlUuid, _>(contact_id)
            .execute(connection)?;

        Ok(())
    })
}

#[async_trait]
impl HostContactStore for PostgresStorage {
    async fn list_host_contacts(
        &self,
        page: &PageRequest,
        filter: &HostContactFilter,
    ) -> Result<Page<HostContact>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_host_contact(
        &self,
        command: CreateHostContact,
    ) -> Result<HostContact, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_host_contact_by_email(
        &self,
        email: &EmailAddressValue,
    ) -> Result<HostContact, AppError> {
        let email = email.as_str().to_string();
        self.database
            .run(move |connection| get_by_email(connection, &email))
            .await
    }

    async fn delete_host_contact(&self, email: &EmailAddressValue) -> Result<(), AppError> {
        let email = email.as_str().to_string();
        self.database
            .run(move |connection| delete(connection, &email))
            .await
    }
}
