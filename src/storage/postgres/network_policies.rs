use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    Connection, OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Bool, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        filters::NetworkPolicyFilter,
        network_policy::{
            CreateNetworkPolicy, CreateNetworkPolicyAttribute, NetworkPolicy,
            NetworkPolicyAttribute, NetworkPolicyAttributeValue, UpdateNetworkPolicy,
            UpdateNetworkPolicyAttribute,
        },
        pagination::{Page, PageRequest},
        types::{NetworkPolicyAttributeName, NetworkPolicyName, UpdateField},
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

#[derive(QueryableByName)]
struct NetworkPolicyAttributeRow {
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
struct NetworkPolicyAttributeValueRow {
    #[diesel(sql_type = SqlUuid)]
    attribute_id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Bool)]
    value: bool,
}

fn row_to_attribute(row: NetworkPolicyAttributeRow) -> Result<NetworkPolicyAttribute, AppError> {
    Ok(NetworkPolicyAttribute::restore(
        row.id,
        NetworkPolicyAttributeName::new(row.name)?,
        row.description,
        row.created_at,
        row.updated_at,
    ))
}

fn replace_values(
    connection: &mut PgConnection,
    policy_id: Uuid,
    requested: &[crate::domain::network_policy::SetNetworkPolicyAttributeValue],
) -> Result<(), AppError> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for (position, requested) in requested.iter().enumerate() {
        if !seen.insert(requested.name().clone()) {
            return Err(AppError::validation(format!(
                "network policy attribute '{}' was provided more than once",
                requested.name()
            )));
        }
        let attribute_id = sql_query("SELECT id FROM network_policy_attributes WHERE name = $1")
            .bind::<Text, _>(requested.name().as_str())
            .get_result::<IdRow>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::validation(format!(
                    "network policy attribute '{}' does not exist",
                    requested.name()
                ))
            })?
            .id;
        sql_query(
            "INSERT INTO network_policy_attribute_values (policy_id, attribute_id, value, position)
             VALUES ($1, $2, $3, $4)",
        )
        .bind::<SqlUuid, _>(policy_id)
        .bind::<SqlUuid, _>(attribute_id)
        .bind::<Bool, _>(requested.value())
        .bind::<diesel::sql_types::Integer, _>(
            i32::try_from(position)
                .map_err(|_| AppError::validation("too many network policy attributes"))?,
        )
        .execute(connection)?;
    }
    Ok(())
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
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
    connection.transaction(|connection| {
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
        replace_values(connection, row.id, command.attributes())?;
        NetworkPolicy::restore(
            row.id,
            NetworkPolicyName::new(&row.name)?,
            row.description,
            row.community_template_pattern,
            row.created_at,
            row.updated_at,
        )
    })
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

pub(super) fn update(
    connection: &mut PgConnection,
    name: &str,
    command: UpdateNetworkPolicy,
) -> Result<NetworkPolicy, AppError> {
    connection.transaction(|connection| {
        let old = get_by_name(connection, name)?;
        let new_name = command.name.unwrap_or_else(|| old.name().clone());
        let description = command
            .description
            .unwrap_or_else(|| old.description().to_string());
        let pattern = match command.community_template_pattern {
            UpdateField::Unchanged => old.community_template_pattern().map(str::to_string),
            UpdateField::Clear => None,
            UpdateField::Set(value) => Some(value),
        };
        let row = sql_query(
            "UPDATE network_policies
             SET name = $2, description = $3, community_template_pattern = $4, updated_at = now()
             WHERE name = $1
             RETURNING id, name::text AS name, description, community_template_pattern,
                       created_at, updated_at",
        )
        .bind::<Text, _>(name)
        .bind::<Text, _>(new_name.as_str())
        .bind::<Text, _>(&description)
        .bind::<Nullable<Text>, _>(&pattern)
        .get_result::<NetworkPolicyRow>(connection)
        .map_err(map_unique(
            "network policy already exists or community template pattern is in use",
        ))?;
        if let Some(values) = command.attributes {
            sql_query("DELETE FROM network_policy_attribute_values WHERE policy_id = $1")
                .bind::<SqlUuid, _>(row.id)
                .execute(connection)?;
            replace_values(connection, row.id, &values)?;
        }
        NetworkPolicy::restore(
            row.id,
            NetworkPolicyName::new(row.name)?,
            row.description,
            row.community_template_pattern,
            row.created_at,
            row.updated_at,
        )
    })
}

pub(super) fn list_attribute_values(
    connection: &mut PgConnection,
    policy: &str,
) -> Result<Vec<NetworkPolicyAttributeValue>, AppError> {
    get_by_name(connection, policy)?;
    sql_query(
        "SELECT a.id AS attribute_id, a.name::text AS name, v.value
         FROM network_policy_attribute_values v
         JOIN network_policies p ON p.id = v.policy_id
         JOIN network_policy_attributes a ON a.id = v.attribute_id
         WHERE p.name = $1
         ORDER BY v.position, a.id",
    )
    .bind::<Text, _>(policy)
    .load::<NetworkPolicyAttributeValueRow>(connection)?
    .into_iter()
    .map(|row| {
        Ok(NetworkPolicyAttributeValue::restore(
            row.attribute_id,
            NetworkPolicyAttributeName::new(row.name)?,
            row.value,
        ))
    })
    .collect()
}

pub(super) fn list_attributes(
    connection: &mut PgConnection,
    page: &PageRequest,
) -> Result<Page<NetworkPolicyAttribute>, AppError> {
    let attributes = sql_query(
        "SELECT id, name::text AS name, description, created_at, updated_at
         FROM network_policy_attributes ORDER BY created_at, id",
    )
    .load::<NetworkPolicyAttributeRow>(connection)?
    .into_iter()
    .map(row_to_attribute)
    .collect::<Result<Vec<_>, _>>()?;
    Ok(vec_to_page(attributes, page))
}

pub(super) fn create_attribute(
    connection: &mut PgConnection,
    command: CreateNetworkPolicyAttribute,
) -> Result<NetworkPolicyAttribute, AppError> {
    let row = sql_query(
        "INSERT INTO network_policy_attributes (name, description)
         VALUES ($1, $2)
         RETURNING id, name::text AS name, description, created_at, updated_at",
    )
    .bind::<Text, _>(command.name().as_str())
    .bind::<Text, _>(command.description())
    .get_result::<NetworkPolicyAttributeRow>(connection)
    .map_err(map_unique("network policy attribute already exists"))?;
    row_to_attribute(row)
}

pub(super) fn get_attribute(
    connection: &mut PgConnection,
    name: &str,
) -> Result<NetworkPolicyAttribute, AppError> {
    let row = sql_query(
        "SELECT id, name::text AS name, description, created_at, updated_at
         FROM network_policy_attributes WHERE name = $1",
    )
    .bind::<Text, _>(name)
    .get_result::<NetworkPolicyAttributeRow>(connection)
    .optional()?
    .ok_or_else(|| {
        AppError::not_found(format!("network policy attribute '{}' was not found", name))
    })?;
    row_to_attribute(row)
}

pub(super) fn update_attribute(
    connection: &mut PgConnection,
    name: &str,
    command: UpdateNetworkPolicyAttribute,
) -> Result<NetworkPolicyAttribute, AppError> {
    let old = get_attribute(connection, name)?;
    let new_name = command.name.unwrap_or_else(|| old.name().clone());
    let description = command
        .description
        .unwrap_or_else(|| old.description().to_string());
    let row = sql_query(
        "UPDATE network_policy_attributes
         SET name = $2, description = $3, updated_at = now()
         WHERE name = $1
         RETURNING id, name::text AS name, description, created_at, updated_at",
    )
    .bind::<Text, _>(name)
    .bind::<Text, _>(new_name.as_str())
    .bind::<Text, _>(description)
    .get_result::<NetworkPolicyAttributeRow>(connection)
    .map_err(map_unique("network policy attribute already exists"))?;
    row_to_attribute(row)
}

pub(super) fn delete_attribute(connection: &mut PgConnection, name: &str) -> Result<(), AppError> {
    let deleted = sql_query("DELETE FROM network_policy_attributes WHERE name = $1")
        .bind::<Text, _>(name)
        .execute(connection)?;
    if deleted == 0 {
        return Err(AppError::not_found(format!(
            "network policy attribute '{}' was not found",
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

    async fn update_network_policy(
        &self,
        name: &NetworkPolicyName,
        command: UpdateNetworkPolicy,
    ) -> Result<NetworkPolicy, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| update(connection, &name, command))
            .await
    }

    async fn delete_network_policy(&self, name: &NetworkPolicyName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| delete(connection, &name))
            .await
    }

    async fn list_network_policy_attribute_values(
        &self,
        policy: &NetworkPolicyName,
    ) -> Result<Vec<NetworkPolicyAttributeValue>, AppError> {
        let policy = policy.as_str().to_string();
        self.database
            .run(move |connection| list_attribute_values(connection, &policy))
            .await
    }

    async fn list_network_policy_attributes(
        &self,
        page: &PageRequest,
    ) -> Result<Page<NetworkPolicyAttribute>, AppError> {
        let page = page.clone();
        self.database
            .run(move |connection| list_attributes(connection, &page))
            .await
    }

    async fn create_network_policy_attribute(
        &self,
        command: CreateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError> {
        self.database
            .run(move |connection| create_attribute(connection, command))
            .await
    }

    async fn get_network_policy_attribute_by_name(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<NetworkPolicyAttribute, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| get_attribute(connection, &name))
            .await
    }

    async fn update_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
        command: UpdateNetworkPolicyAttribute,
    ) -> Result<NetworkPolicyAttribute, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| update_attribute(connection, &name, command))
            .await
    }

    async fn delete_network_policy_attribute(
        &self,
        name: &NetworkPolicyAttributeName,
    ) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |connection| delete_attribute(connection, &name))
            .await
    }
}
