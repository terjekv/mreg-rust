use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{CreateNetworkPolicy, NetworkPolicy},
        pagination::{Page, PageRequest},
        types::NetworkPolicyName,
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, run_dynamic_query, vec_to_page},
    storage::{NetworkPolicyStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct NetworkPolicyRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    description: String,
    #[diesel(sql_type = Nullable<Text>)]
    community_template_pattern: Option<String>,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &NetworkPolicyFilter,
) -> Result<Page<NetworkPolicy>, AppError> {
    let base = "SELECT np.id, np.name::text AS name, np.description, \
                np.community_template_pattern, \
                np.created_at, np.updated_at \
                FROM network_policies np";

    let (clauses, values) = filter.sql_conditions();
    let where_str = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let query_str = format!("{base}{where_str} ORDER BY np.name");

    let rows = run_dynamic_query::<NetworkPolicyRow>(connection, &query_str, &values)?;

    let all: Vec<NetworkPolicy> = rows
        .into_iter()
        .map(|row| {
            NetworkPolicy::restore(
                row.id,
                NetworkPolicyName::new(&row.name)?,
                row.description,
                row.community_template_pattern,
                row.created_at,
                row.updated_at,
            )
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    // Apply special filters (search) in Rust
    let items: Vec<NetworkPolicy> = all
        .into_iter()
        .filter(|policy| filter.matches(policy))
        .collect();

    Ok(vec_to_page(items, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreateNetworkPolicy,
) -> Result<NetworkPolicy, AppError> {
    let row = sql_query(
        "INSERT INTO network_policies (name, description, community_template_pattern)
         VALUES ($1, $2, $3)
         RETURNING id, name::text AS name, description,
                   community_template_pattern,
                   created_at, updated_at",
    )
    .bind::<Text, _>(command.name().as_str())
    .bind::<Text, _>(command.description())
    .bind::<Nullable<Text>, _>(
        command
            .community_template_pattern()
            .map(|pattern| pattern.to_string()),
    )
    .get_result::<NetworkPolicyRow>(connection)
    .map_err(map_unique("network policy already exists"))?;

    NetworkPolicy::restore(
        row.id,
        NetworkPolicyName::new(&row.name)?,
        row.description,
        row.community_template_pattern,
        row.created_at,
        row.updated_at,
    )
}

pub(super) fn get_by_name(
    connection: &mut PgConnection,
    name: &str,
) -> Result<NetworkPolicy, AppError> {
    let row = sql_query(
        "SELECT id, name::text AS name, description,
                community_template_pattern,
                created_at, updated_at
         FROM network_policies
         WHERE name = $1",
    )
    .bind::<Text, _>(name)
    .get_result::<NetworkPolicyRow>(connection)
    .optional()?
    .ok_or_else(|| AppError::not_found(format!("network policy '{}' was not found", name)))?;

    NetworkPolicy::restore(
        row.id,
        NetworkPolicyName::new(&row.name)?,
        row.description,
        row.community_template_pattern,
        row.created_at,
        row.updated_at,
    )
}

pub(super) fn delete(connection: &mut PgConnection, name: &str) -> Result<(), AppError> {
    let deleted = sql_query("DELETE FROM network_policies WHERE name = $1")
        .bind::<Text, _>(name)
        .execute(connection)
        .map_err(|error| match error {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                _,
            ) => AppError::conflict("network policy is still referenced by other resources"),
            other => AppError::internal(other),
        })?;
    if deleted == 0 {
        return Err(AppError::not_found(format!(
            "network policy '{}' was not found",
            name
        )));
    }
    Ok(())
}

#[async_trait]
impl NetworkPolicyStore for PostgresStorage {
    async fn list_network_policies(
        &self,
        page: &PageRequest,
        filter: &NetworkPolicyFilter,
    ) -> Result<Page<NetworkPolicy>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_network_policy(
        &self,
        command: CreateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_network_policy_by_name(
        &self,
        name: &NetworkPolicyName,
    ) -> Result<NetworkPolicy, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| get_by_name(connection, &name))
            .await
    }

    async fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| delete(connection, &name))
            .await
    }
}
