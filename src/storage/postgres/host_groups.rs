use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, QueryableByName,
    RunQueryDsl, sql_query,
    sql_types::{Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        filters::HostGroupFilter,
        host_group::{CreateHostGroup, HostGroup},
        pagination::{Page, PageRequest},
        types::{HostGroupName, Hostname, OwnerGroupName},
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, run_dynamic_query, vec_to_page},
    storage::{HostGroupStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct HostGroupRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    description: String,
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
struct ParentGroupNameRow {
    #[diesel(sql_type = Text)]
    parent_name: String,
}

#[derive(QueryableByName)]
struct OwnerGroupRow {
    #[diesel(sql_type = Text)]
    owner_group: String,
}

fn load_group_hosts(
    connection: &mut PgConnection,
    group_id: Uuid,
) -> Result<Vec<Hostname>, AppError> {
    let rows = sql_query(
        "SELECT h.name::text AS host_name
         FROM host_group_hosts hgh
         JOIN hosts h ON h.id = hgh.host_id
         WHERE hgh.host_group_id = $1
         ORDER BY h.name",
    )
    .bind::<SqlUuid, _>(group_id)
    .load::<JunctionHostNameRow>(connection)?;

    rows.into_iter()
        .map(|row| Hostname::new(row.host_name))
        .collect()
}

fn load_group_parents(
    connection: &mut PgConnection,
    group_id: Uuid,
) -> Result<Vec<HostGroupName>, AppError> {
    let rows = sql_query(
        "SELECT pg.name::text AS parent_name
         FROM host_group_parents hgp
         JOIN host_groups pg ON pg.id = hgp.parent_group_id
         WHERE hgp.host_group_id = $1
         ORDER BY pg.name",
    )
    .bind::<SqlUuid, _>(group_id)
    .load::<ParentGroupNameRow>(connection)?;

    rows.into_iter()
        .map(|row| HostGroupName::new(row.parent_name))
        .collect()
}

fn load_group_owner_groups(
    connection: &mut PgConnection,
    group_id: Uuid,
) -> Result<Vec<OwnerGroupName>, AppError> {
    let rows = sql_query(
        "SELECT owner_group::text AS owner_group
         FROM host_group_owner_groups
         WHERE host_group_id = $1
         ORDER BY owner_group",
    )
    .bind::<SqlUuid, _>(group_id)
    .load::<OwnerGroupRow>(connection)?;

    rows.into_iter()
        .map(|row| OwnerGroupName::new(row.owner_group))
        .collect()
}

fn build_host_group(
    connection: &mut PgConnection,
    row: HostGroupRow,
) -> Result<HostGroup, AppError> {
    let hosts = load_group_hosts(connection, row.id)?;
    let parents = load_group_parents(connection, row.id)?;
    let owners = load_group_owner_groups(connection, row.id)?;
    HostGroup::restore(
        row.id,
        HostGroupName::new(&row.name)?,
        row.description,
        hosts,
        parents,
        owners,
        row.created_at,
        row.updated_at,
    )
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &HostGroupFilter,
) -> Result<Page<HostGroup>, AppError> {
    let base = "SELECT hg.id, hg.name::text AS name, hg.description, \
                hg.created_at, hg.updated_at \
                FROM host_groups hg";

    let (clauses, values) = filter.sql_conditions();
    let where_str = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let query_str = format!("{base}{where_str} ORDER BY hg.name");

    let rows = run_dynamic_query::<HostGroupRow>(connection, &query_str, &values)?;

    let all: Vec<HostGroup> = rows
        .into_iter()
        .map(|row| build_host_group(connection, row))
        .collect::<Result<Vec<_>, _>>()?;

    // Apply special filters (host, search) in Rust
    let items: Vec<HostGroup> = all
        .into_iter()
        .filter(|group| filter.matches(group))
        .collect();

    Ok(vec_to_page(items, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreateHostGroup,
) -> Result<HostGroup, AppError> {
    connection.transaction::<HostGroup, AppError, _>(|connection| {
        let row = sql_query(
            "INSERT INTO host_groups (name, description)
             VALUES ($1, $2)
             RETURNING id, name::text AS name, description,
                       created_at, updated_at",
        )
        .bind::<Text, _>(command.name().as_str())
        .bind::<Text, _>(command.description())
        .get_result::<HostGroupRow>(connection)
        .map_err(map_unique("host group already exists"))?;

        let group_id = row.id;

        // Link hosts
        for host_name in command.hosts() {
            let host_id = PostgresStorage::resolve_host_id(connection, host_name)?;
            sql_query(
                "INSERT INTO host_group_hosts (host_group_id, host_id)
                 VALUES ($1, $2)",
            )
            .bind::<SqlUuid, _>(group_id)
            .bind::<SqlUuid, _>(host_id)
            .execute(connection)?;
        }

        // Link parent groups
        for parent_name in command.parent_groups() {
            let parent_id = PostgresStorage::resolve_host_group_id(connection, parent_name)?;
            sql_query(
                "INSERT INTO host_group_parents (host_group_id, parent_group_id)
                 VALUES ($1, $2)",
            )
            .bind::<SqlUuid, _>(group_id)
            .bind::<SqlUuid, _>(parent_id)
            .execute(connection)?;
        }

        // Link owner groups
        for owner_name in command.owner_groups() {
            sql_query(
                "INSERT INTO host_group_owner_groups (host_group_id, owner_group)
                 VALUES ($1, $2)",
            )
            .bind::<SqlUuid, _>(group_id)
            .bind::<Text, _>(owner_name.as_str())
            .execute(connection)?;
        }

        build_host_group(connection, row)
    })
}

pub(super) fn get_by_name(
    connection: &mut PgConnection,
    name: &str,
) -> Result<HostGroup, AppError> {
    let row = sql_query(
        "SELECT id, name::text AS name, description,
                created_at, updated_at
         FROM host_groups
         WHERE name = $1",
    )
    .bind::<Text, _>(name)
    .get_result::<HostGroupRow>(connection)
    .optional()?
    .ok_or_else(|| AppError::not_found(format!("host group '{}' was not found", name)))?;

    build_host_group(connection, row)
}

pub(super) fn delete(connection: &mut PgConnection, name: &str) -> Result<(), AppError> {
    connection.transaction::<(), AppError, _>(|connection| {
        use crate::db::schema::host_groups;

        let group_id = host_groups::table
            .filter(host_groups::name.eq(name))
            .select(host_groups::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found(format!("host group '{}' was not found", name)))?;

        sql_query("DELETE FROM host_group_hosts WHERE host_group_id = $1")
            .bind::<SqlUuid, _>(group_id)
            .execute(connection)?;

        sql_query("DELETE FROM host_group_parents WHERE host_group_id = $1")
            .bind::<SqlUuid, _>(group_id)
            .execute(connection)?;

        sql_query("DELETE FROM host_group_owner_groups WHERE host_group_id = $1")
            .bind::<SqlUuid, _>(group_id)
            .execute(connection)?;

        sql_query("DELETE FROM host_groups WHERE id = $1")
            .bind::<SqlUuid, _>(group_id)
            .execute(connection)?;

        Ok(())
    })
}

#[async_trait]
impl HostGroupStore for PostgresStorage {
    async fn list_host_groups(
        &self,
        page: &PageRequest,
        filter: &HostGroupFilter,
    ) -> Result<Page<HostGroup>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_host_group(&self, command: CreateHostGroup) -> Result<HostGroup, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_host_group_by_name(&self, name: &HostGroupName) -> Result<HostGroup, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| get_by_name(connection, &name))
            .await
    }

    async fn delete_host_group(&self, name: &HostGroupName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| delete(connection, &name))
            .await
    }
}
