use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    Connection, OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        community::{Community, CreateCommunity},
        filters::CommunityFilter,
        pagination::{Page, PageRequest},
        types::{CidrValue, CommunityName, NetworkPolicyName},
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, run_dynamic_query, vec_to_page},
    storage::{CommunityStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct CommunityRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    policy_id: Uuid,
    #[diesel(sql_type = Text)]
    policy_name: String,
    #[diesel(sql_type = Text)]
    network_cidr: String,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    description: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

fn row_to_community(row: CommunityRow) -> Result<Community, AppError> {
    Community::restore(
        row.id,
        row.policy_id,
        NetworkPolicyName::new(&row.policy_name)?,
        CidrValue::new(row.network_cidr)?,
        CommunityName::new(&row.name)?,
        row.description,
        row.created_at,
        row.updated_at,
    )
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &CommunityFilter,
) -> Result<Page<Community>, AppError> {
    let base = "SELECT c.id, c.policy_id, \
                np.name::text AS policy_name, \
                n.network::text AS network_cidr, \
                c.name::text AS name, c.description, \
                c.created_at, c.updated_at \
                FROM communities c \
                JOIN network_policies np ON np.id = c.policy_id \
                JOIN networks n ON n.id = c.network_id";

    let (clauses, values) = filter.sql_conditions();
    let where_str = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };
    let order_col = match page.sort_by() {
        Some("policy_name") => "np.name::text",
        Some("created_at") => "c.created_at",
        _ => "c.name::text",
    };
    let order_dir = match page.sort_direction() {
        crate::domain::pagination::SortDirection::Asc => "ASC",
        crate::domain::pagination::SortDirection::Desc => "DESC",
    };
    let query_str = format!("{base}{where_str} ORDER BY {order_col} {order_dir}, c.id");

    let rows = run_dynamic_query::<CommunityRow>(connection, &query_str, &values)?;

    let all: Vec<Community> = rows
        .into_iter()
        .map(row_to_community)
        .collect::<Result<Vec<_>, AppError>>()?;

    // Apply special filters (network, search) in Rust
    let items: Vec<Community> = all
        .into_iter()
        .filter(|community| filter.matches(community))
        .collect();

    Ok(vec_to_page(items, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreateCommunity,
) -> Result<Community, AppError> {
    connection.transaction::<Community, AppError, _>(|connection| {
        // Resolve policy
        let policy_id =
            PostgresStorage::resolve_network_policy_id(connection, command.policy_name())?;

        // Resolve network
        let network = PostgresStorage::query_network_by_cidr(connection, command.network_cidr())?;

        let row = sql_query(
            "INSERT INTO communities (policy_id, network_id, name, description)
             VALUES ($1, $2, $3, $4)
             RETURNING id, $1 AS policy_id,
                       $5::text AS policy_name,
                       $6::text AS network_cidr,
                       name::text AS name, description,
                       created_at, updated_at",
        )
        .bind::<SqlUuid, _>(policy_id)
        .bind::<SqlUuid, _>(network.id())
        .bind::<Text, _>(command.name().as_str())
        .bind::<Text, _>(command.description())
        .bind::<Text, _>(command.policy_name().as_str())
        .bind::<Text, _>(command.network_cidr().as_str())
        .get_result::<CommunityRow>(connection)
        .map_err(map_unique("community already exists"))?;

        row_to_community(row)
    })
}

pub(super) fn get_by_id(
    connection: &mut PgConnection,
    community_id: Uuid,
) -> Result<Community, AppError> {
    let row = sql_query(
        "SELECT c.id, c.policy_id,
                np.name::text AS policy_name,
                n.network::text AS network_cidr,
                c.name::text AS name, c.description,
                c.created_at, c.updated_at
         FROM communities c
         JOIN network_policies np ON np.id = c.policy_id
         JOIN networks n ON n.id = c.network_id
         WHERE c.id = $1",
    )
    .bind::<SqlUuid, _>(community_id)
    .get_result::<CommunityRow>(connection)
    .optional()?
    .ok_or_else(|| AppError::not_found("community was not found"))?;

    row_to_community(row)
}

pub(super) fn delete_by_id(
    connection: &mut PgConnection,
    community_id: Uuid,
) -> Result<(), AppError> {
    let deleted = sql_query("DELETE FROM communities WHERE id = $1")
        .bind::<SqlUuid, _>(community_id)
        .execute(connection)
        .map_err(|error| match error {
            diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                _,
            ) => AppError::conflict("community is still referenced by host community assignments"),
            other => AppError::internal(other),
        })?;
    if deleted == 0 {
        return Err(AppError::not_found("community was not found"));
    }
    Ok(())
}

pub(in crate::storage::postgres) fn find_by_names(
    connection: &mut PgConnection,
    policy_name: &str,
    community_name: &str,
) -> Result<Community, AppError> {
    let row = sql_query(
        "SELECT c.id, c.policy_id,
                np.name::text AS policy_name,
                n.network::text AS network_cidr,
                c.name::text AS name, c.description,
                c.created_at, c.updated_at
         FROM communities c
         JOIN network_policies np ON np.id = c.policy_id
         JOIN networks n ON n.id = c.network_id
         WHERE np.name = $1
           AND c.name = $2",
    )
    .bind::<Text, _>(policy_name)
    .bind::<Text, _>(community_name)
    .get_result::<CommunityRow>(connection)
    .optional()?
    .ok_or_else(|| {
        AppError::not_found(format!(
            "community '{}/{}' was not found",
            policy_name, community_name
        ))
    })?;

    row_to_community(row)
}

#[async_trait]
impl CommunityStore for PostgresStorage {
    async fn list_communities(
        &self,
        page: &PageRequest,
        filter: &CommunityFilter,
    ) -> Result<Page<Community>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_community(&self, command: CreateCommunity) -> Result<Community, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_community(&self, community_id: Uuid) -> Result<Community, AppError> {
        self.database
            .run(move |connection| get_by_id(connection, community_id))
            .await
    }

    async fn delete_community(&self, community_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| delete_by_id(connection, community_id))
            .await
    }

    async fn find_community_by_names(
        &self,
        policy_name: &NetworkPolicyName,
        community_name: &CommunityName,
    ) -> Result<Community, AppError> {
        let pn = policy_name.as_str().to_string();
        let cn = community_name.as_str().to_string();
        self.database
            .run(move |connection| find_by_names(connection, &pn, &cn))
            .await
    }
}
