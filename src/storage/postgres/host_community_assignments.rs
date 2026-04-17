use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    Connection, OptionalExtension, PgConnection, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        filters::HostCommunityAssignmentFilter,
        host_community_assignment::{CreateHostCommunityAssignment, HostCommunityAssignment},
        pagination::{Page, PageRequest},
        types::{CommunityName, Hostname, IpAddressValue, NetworkPolicyName},
    },
    errors::AppError,
    storage::postgres::helpers::{map_unique, run_dynamic_query, vec_to_page},
    storage::{HostCommunityAssignmentStore, postgres::PostgresStorage},
};

#[derive(QueryableByName)]
struct HostCommunityAssignmentRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = SqlUuid)]
    host_id: Uuid,
    #[diesel(sql_type = Text)]
    host_name: String,
    #[diesel(sql_type = SqlUuid)]
    ip_address_id: Uuid,
    #[diesel(sql_type = Text)]
    address: String,
    #[diesel(sql_type = SqlUuid)]
    community_id: Uuid,
    #[diesel(sql_type = Text)]
    community_name: String,
    #[diesel(sql_type = Text)]
    policy_name: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
}

fn row_to_mapping(row: HostCommunityAssignmentRow) -> Result<HostCommunityAssignment, AppError> {
    Ok(HostCommunityAssignment::restore(
        row.id,
        row.host_id,
        Hostname::new(row.host_name)?,
        row.ip_address_id,
        IpAddressValue::new(row.address)?,
        row.community_id,
        CommunityName::new(&row.community_name)?,
        NetworkPolicyName::new(&row.policy_name)?,
        row.created_at,
        row.updated_at,
    ))
}

pub(super) fn list(
    connection: &mut PgConnection,
    page: &PageRequest,
    filter: &HostCommunityAssignmentFilter,
) -> Result<Page<HostCommunityAssignment>, AppError> {
    let (clauses, values) = filter.sql_conditions();
    let mut query = String::from(
        "SELECT m.id, m.host_id,
                h.name::text AS host_name,
                m.ip_address_id,
                host(ip.address) AS address,
                m.community_id,
                c.name::text AS community_name,
                np.name::text AS policy_name,
                m.created_at, m.updated_at
         FROM host_community_assignments m
         JOIN hosts h ON h.id = m.host_id
         JOIN ip_addresses ip ON ip.id = m.ip_address_id
         JOIN communities c ON c.id = m.community_id
         JOIN network_policies np ON np.id = c.policy_id",
    );
    if !clauses.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&clauses.join(" AND "));
    }
    query.push_str(" ORDER BY h.name, ip.address");
    let rows = run_dynamic_query::<HostCommunityAssignmentRow>(connection, &query, &values)?;

    let items: Vec<HostCommunityAssignment> = rows
        .into_iter()
        .map(row_to_mapping)
        .collect::<Result<Vec<_>, AppError>>()?;

    Ok(vec_to_page(items, page))
}

pub(in crate::storage::postgres) fn create(
    connection: &mut PgConnection,
    command: CreateHostCommunityAssignment,
) -> Result<HostCommunityAssignment, AppError> {
    connection.transaction::<HostCommunityAssignment, AppError, _>(|connection| {
        let host_id = PostgresStorage::resolve_host_id(connection, command.host_name())?;

        // Resolve ip_address_id from the address + host_id
        let ip_row = sql_query(
            "SELECT id FROM ip_addresses
             WHERE host_id = $1 AND address = $2::inet",
        )
        .bind::<SqlUuid, _>(host_id)
        .bind::<Text, _>(command.address().as_str())
        .get_result::<crate::db::models::UuidRow>(connection)
        .optional()?
        .ok_or_else(|| {
            AppError::not_found(format!(
                "IP address '{}' not assigned to host '{}'",
                command.address().as_str(),
                command.host_name().as_str()
            ))
        })?;

        // Resolve community by policy_name + community_name
        let community_row = sql_query(
            "SELECT c.id
             FROM communities c
             JOIN network_policies np ON np.id = c.policy_id
             WHERE np.name = $1 AND c.name = $2",
        )
        .bind::<Text, _>(command.policy_name().as_str())
        .bind::<Text, _>(command.community_name().as_str())
        .get_result::<crate::db::models::UuidRow>(connection)
        .optional()?
        .ok_or_else(|| {
            AppError::not_found(format!(
                "community '{}/{}' was not found",
                command.policy_name().as_str(),
                command.community_name().as_str()
            ))
        })?;

        let row = sql_query(
            "INSERT INTO host_community_assignments
                (host_id, ip_address_id, community_id)
             VALUES ($1, $2, $3)
             RETURNING id, host_id,
                       $4::text AS host_name,
                       ip_address_id,
                       $5::text AS address,
                       community_id,
                       $6::text AS community_name,
                       $7::text AS policy_name,
                       created_at, updated_at",
        )
        .bind::<SqlUuid, _>(host_id)
        .bind::<SqlUuid, _>(ip_row.id())
        .bind::<SqlUuid, _>(community_row.id())
        .bind::<Text, _>(command.host_name().as_str())
        .bind::<Text, _>(command.address().as_str())
        .bind::<Text, _>(command.community_name().as_str())
        .bind::<Text, _>(command.policy_name().as_str())
        .get_result::<HostCommunityAssignmentRow>(connection)
        .map_err(map_unique("host community assignment already exists"))?;

        row_to_mapping(row)
    })
}

pub(super) fn get_by_id(
    connection: &mut PgConnection,
    mapping_id: Uuid,
) -> Result<HostCommunityAssignment, AppError> {
    let row = sql_query(
        "SELECT m.id, m.host_id,
                h.name::text AS host_name,
                m.ip_address_id,
                host(ip.address) AS address,
                m.community_id,
                c.name::text AS community_name,
                np.name::text AS policy_name,
                m.created_at, m.updated_at
         FROM host_community_assignments m
         JOIN hosts h ON h.id = m.host_id
         JOIN ip_addresses ip ON ip.id = m.ip_address_id
         JOIN communities c ON c.id = m.community_id
         JOIN network_policies np ON np.id = c.policy_id
         WHERE m.id = $1",
    )
    .bind::<SqlUuid, _>(mapping_id)
    .get_result::<HostCommunityAssignmentRow>(connection)
    .optional()?
    .ok_or_else(|| AppError::not_found("host community assignment was not found"))?;

    row_to_mapping(row)
}

pub(super) fn delete_by_id(
    connection: &mut PgConnection,
    mapping_id: Uuid,
) -> Result<(), AppError> {
    let deleted = sql_query("DELETE FROM host_community_assignments WHERE id = $1")
        .bind::<SqlUuid, _>(mapping_id)
        .execute(connection)?;
    if deleted == 0 {
        return Err(AppError::not_found(
            "host community assignment was not found",
        ));
    }
    Ok(())
}

#[async_trait]
impl HostCommunityAssignmentStore for PostgresStorage {
    async fn list_host_community_assignments(
        &self,
        page: &PageRequest,
        filter: &HostCommunityAssignmentFilter,
    ) -> Result<Page<HostCommunityAssignment>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| list(connection, &page, &filter))
            .await
    }

    async fn create_host_community_assignment(
        &self,
        command: CreateHostCommunityAssignment,
    ) -> Result<HostCommunityAssignment, AppError> {
        self.database
            .run(move |connection| create(connection, command))
            .await
    }

    async fn get_host_community_assignment(
        &self,
        mapping_id: Uuid,
    ) -> Result<HostCommunityAssignment, AppError> {
        self.database
            .run(move |connection| get_by_id(connection, mapping_id))
            .await
    }

    async fn delete_host_community_assignment(&self, mapping_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| delete_by_id(connection, mapping_id))
            .await
    }
}
