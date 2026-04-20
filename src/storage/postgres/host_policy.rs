use std::collections::HashMap;

use async_trait::async_trait;
use diesel::{
    ExpressionMethods, QueryDsl, QueryableByName, RunQueryDsl, sql_query,
    sql_types::{Text, Timestamptz, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    domain::{
        host_policy::{
            CreateHostPolicyAtom, CreateHostPolicyRole, HostPolicyAtom, HostPolicyRole,
            UpdateHostPolicyAtom, UpdateHostPolicyRole,
        },
        pagination::{Page, PageRequest},
        types::{HostPolicyName, Hostname},
    },
    errors::AppError,
    storage::HostPolicyStore,
};

use super::PostgresStorage;
use super::helpers::vec_to_page;

#[derive(QueryableByName)]
struct AtomRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    description: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: chrono::DateTime<chrono::Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl AtomRow {
    fn into_domain(self) -> Result<HostPolicyAtom, AppError> {
        Ok(HostPolicyAtom::restore(
            self.id,
            HostPolicyName::new(self.name)?,
            self.description,
            self.created_at,
            self.updated_at,
        ))
    }
}

#[derive(QueryableByName)]
struct RoleRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    description: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: chrono::DateTime<chrono::Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(QueryableByName)]
struct NameRow {
    #[diesel(sql_type = Text)]
    name: String,
}

fn map_fk_violation(message: &'static str) -> impl FnOnce(diesel::result::Error) -> AppError {
    move |error| match error {
        diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::ForeignKeyViolation,
            _,
        ) => AppError::conflict(message),
        diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        ) => AppError::conflict(message),
        other => AppError::internal(other),
    }
}

fn map_unique(message: &'static str) -> impl FnOnce(diesel::result::Error) -> AppError {
    move |error| match error {
        diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            _,
        ) => AppError::conflict(message),
        other => AppError::internal(other),
    }
}

#[async_trait]
impl HostPolicyStore for PostgresStorage {
    async fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let rows = sql_query(
                    "SELECT id, name, description, created_at, updated_at
                     FROM host_policy_atoms ORDER BY name ASC",
                )
                .load::<AtomRow>(c)?;
                let items: Result<Vec<_>, _> = rows.into_iter().map(AtomRow::into_domain).collect();
                Ok(vec_to_page(items?, &page))
            })
            .await
    }

    async fn create_atom(&self, command: CreateHostPolicyAtom) -> Result<HostPolicyAtom, AppError> {
        let name = command.name().as_str().to_string();
        let description = command.description().to_string();
        self.database
            .run(move |c| {
                let row = sql_query(
                    "INSERT INTO host_policy_atoms (name, description)
                     VALUES ($1, $2)
                     RETURNING id, name, description, created_at, updated_at",
                )
                .bind::<Text, _>(&name)
                .bind::<Text, _>(&description)
                .get_result::<AtomRow>(c)
                .map_err(map_unique("host policy atom already exists"))?;
                row.into_domain()
            })
            .await
    }

    async fn get_atom_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| {
                let row = sql_query(
                    "SELECT id, name, description, created_at, updated_at
                     FROM host_policy_atoms WHERE name = $1",
                )
                .bind::<Text, _>(&name)
                .get_results::<AtomRow>(c)?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    AppError::not_found(format!("host policy atom '{}' was not found", name))
                })?;
                row.into_domain()
            })
            .await
    }

    async fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| {
                if let Some(ref description) = command.description {
                    let row = sql_query(
                        "UPDATE host_policy_atoms SET description = $2, updated_at = now()
                         WHERE name = $1
                         RETURNING id, name, description, created_at, updated_at",
                    )
                    .bind::<Text, _>(&name)
                    .bind::<Text, _>(description)
                    .get_results::<AtomRow>(c)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        AppError::not_found(format!("host policy atom '{}' was not found", name))
                    })?;
                    row.into_domain()
                } else {
                    let row = sql_query(
                        "SELECT id, name, description, created_at, updated_at
                         FROM host_policy_atoms WHERE name = $1",
                    )
                    .bind::<Text, _>(&name)
                    .get_results::<AtomRow>(c)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        AppError::not_found(format!("host policy atom '{}' was not found", name))
                    })?;
                    row.into_domain()
                }
            })
            .await
    }

    async fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| {
                let result = sql_query("DELETE FROM host_policy_atoms WHERE name = $1")
                    .bind::<Text, _>(&name)
                    .execute(c)
                    .map_err(map_fk_violation(
                        "host policy atom is in use by a role and cannot be deleted",
                    ))?;
                if result == 0 {
                    return Err(AppError::not_found(format!(
                        "host policy atom '{}' was not found",
                        name
                    )));
                }
                Ok(())
            })
            .await
    }

    async fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let rows = sql_query(
                    "SELECT id, name, description, created_at, updated_at
                     FROM host_policy_roles ORDER BY name ASC",
                )
                .load::<RoleRow>(c)?;
                Ok(vec_to_page(build_roles_from_rows(c, rows)?, &page))
            })
            .await
    }

    async fn list_roles_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        let host_name = host_name.clone();
        self.database
            .run(move |c| {
                let rows = sql_query(
                    "SELECT DISTINCT r.id, r.name, r.description, r.created_at, r.updated_at
                     FROM host_policy_roles r
                     JOIN host_policy_role_hosts rh ON rh.role_id = r.id
                     JOIN hosts h ON h.id = rh.host_id
                     WHERE h.name = $1
                     ORDER BY r.name ASC",
                )
                .bind::<Text, _>(host_name.as_str())
                .load::<RoleRow>(c)?;
                build_roles_from_rows(c, rows)
            })
            .await
    }

    async fn list_roles_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        let host_names = hosts
            .iter()
            .map(|host| host.as_str().to_string())
            .collect::<Vec<_>>();
        self.database
            .run(move |c| {
                if host_names.is_empty() {
                    return Ok(Vec::new());
                }
                let rows = sql_query(
                    "SELECT DISTINCT r.id, r.name, r.description, r.created_at, r.updated_at
                     FROM host_policy_roles r
                     JOIN host_policy_role_hosts rh ON rh.role_id = r.id
                     JOIN hosts h ON h.id = rh.host_id
                     WHERE h.name = ANY($1::text[])
                     ORDER BY r.name ASC",
                )
                .bind::<diesel::sql_types::Array<Text>, _>(&host_names)
                .load::<RoleRow>(c)?;
                build_roles_from_rows(c, rows)
            })
            .await
    }

    async fn create_role(&self, command: CreateHostPolicyRole) -> Result<HostPolicyRole, AppError> {
        let name = command.name().as_str().to_string();
        let description = command.description().to_string();
        self.database
            .run(move |c| {
                let row = sql_query(
                    "INSERT INTO host_policy_roles (name, description)
                     VALUES ($1, $2)
                     RETURNING id, name, description, created_at, updated_at",
                )
                .bind::<Text, _>(&name)
                .bind::<Text, _>(&description)
                .get_result::<RoleRow>(c)
                .map_err(map_unique("host policy role already exists"))?;
                build_role_from_row(c, row)
            })
            .await
    }

    async fn get_role_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| {
                let row = sql_query(
                    "SELECT id, name, description, created_at, updated_at
                     FROM host_policy_roles WHERE name = $1",
                )
                .bind::<Text, _>(&name)
                .get_results::<RoleRow>(c)?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    AppError::not_found(format!("host policy role '{}' was not found", name))
                })?;
                build_role_from_row(c, row)
            })
            .await
    }

    async fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| {
                let row = if let Some(ref description) = command.description {
                    sql_query(
                        "UPDATE host_policy_roles SET description = $2, updated_at = now()
                         WHERE name = $1
                         RETURNING id, name, description, created_at, updated_at",
                    )
                    .bind::<Text, _>(&name)
                    .bind::<Text, _>(description)
                    .get_results::<RoleRow>(c)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        AppError::not_found(format!("host policy role '{}' was not found", name))
                    })?
                } else {
                    sql_query(
                        "SELECT id, name, description, created_at, updated_at
                         FROM host_policy_roles WHERE name = $1",
                    )
                    .bind::<Text, _>(&name)
                    .get_results::<RoleRow>(c)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        AppError::not_found(format!("host policy role '{}' was not found", name))
                    })?
                };
                build_role_from_row(c, row)
            })
            .await
    }

    async fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| {
                let result = sql_query("DELETE FROM host_policy_roles WHERE name = $1")
                    .bind::<Text, _>(&name)
                    .execute(c)?;
                if result == 0 {
                    return Err(AppError::not_found(format!(
                        "host policy role '{}' was not found",
                        name
                    )));
                }
                Ok(())
            })
            .await
    }

    async fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        let role_name = role_name.as_str().to_string();
        let atom_name = atom_name.as_str().to_string();
        self.database
            .run(move |c| {
                sql_query(
                    "INSERT INTO host_policy_role_atoms (role_id, atom_id)
                     SELECT r.id, a.id
                     FROM host_policy_roles r, host_policy_atoms a
                     WHERE r.name = $1 AND a.name = $2",
                )
                .bind::<Text, _>(&role_name)
                .bind::<Text, _>(&atom_name)
                .execute(c)
                .map_err(map_fk_violation(
                    "atom or role not found, or already assigned",
                ))?;
                Ok(())
            })
            .await
    }

    async fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        let role_name = role_name.as_str().to_string();
        let atom_name = atom_name.as_str().to_string();
        self.database
            .run(move |c| {
                let deleted = sql_query(
                    "DELETE FROM host_policy_role_atoms
                     WHERE role_id = (SELECT id FROM host_policy_roles WHERE name = $1)
                       AND atom_id = (SELECT id FROM host_policy_atoms WHERE name = $2)",
                )
                .bind::<Text, _>(&role_name)
                .bind::<Text, _>(&atom_name)
                .execute(c)?;
                if deleted == 0 {
                    return Err(AppError::not_found(format!(
                        "atom '{}' is not in role '{}'",
                        atom_name, role_name
                    )));
                }
                Ok(())
            })
            .await
    }

    async fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        let role_name = role_name.as_str().to_string();
        let host_name = host_name.to_string();
        self.database
            .run(move |c| {
                sql_query(
                    "INSERT INTO host_policy_role_hosts (role_id, host_id)
                     SELECT r.id, h.id
                     FROM host_policy_roles r, hosts h
                     WHERE r.name = $1 AND h.name = $2",
                )
                .bind::<Text, _>(&role_name)
                .bind::<Text, _>(&host_name)
                .execute(c)
                .map_err(map_fk_violation(
                    "host or role not found, or already assigned",
                ))?;
                Ok(())
            })
            .await
    }

    async fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        let role_name = role_name.as_str().to_string();
        let host_name = host_name.to_string();
        self.database
            .run(move |c| {
                let deleted = sql_query(
                    "DELETE FROM host_policy_role_hosts
                     WHERE role_id = (SELECT id FROM host_policy_roles WHERE name = $1)
                       AND host_id = (SELECT id FROM hosts WHERE name = $2)",
                )
                .bind::<Text, _>(&role_name)
                .bind::<Text, _>(&host_name)
                .execute(c)?;
                if deleted == 0 {
                    return Err(AppError::not_found(format!(
                        "host '{}' is not in role '{}'",
                        host_name, role_name
                    )));
                }
                Ok(())
            })
            .await
    }

    async fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        let role_name = role_name.as_str().to_string();
        let label_name = label_name.to_string();
        self.database
            .run(move |c| {
                sql_query(
                    "INSERT INTO host_policy_role_labels (role_id, label_id)
                     SELECT r.id, l.id
                     FROM host_policy_roles r, labels l
                     WHERE r.name = $1 AND l.name = $2",
                )
                .bind::<Text, _>(&role_name)
                .bind::<Text, _>(&label_name)
                .execute(c)
                .map_err(map_fk_violation(
                    "label or role not found, or already assigned",
                ))?;
                Ok(())
            })
            .await
    }

    async fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        let role_name = role_name.as_str().to_string();
        let label_name = label_name.to_string();
        self.database
            .run(move |c| {
                let deleted = sql_query(
                    "DELETE FROM host_policy_role_labels
                     WHERE role_id = (SELECT id FROM host_policy_roles WHERE name = $1)
                       AND label_id = (SELECT id FROM labels WHERE name = $2)",
                )
                .bind::<Text, _>(&role_name)
                .bind::<Text, _>(&label_name)
                .execute(c)?;
                if deleted == 0 {
                    return Err(AppError::not_found(format!(
                        "label '{}' is not in role '{}'",
                        label_name, role_name
                    )));
                }
                Ok(())
            })
            .await
    }
}

fn build_role_from_row(
    c: &mut diesel::PgConnection,
    row: RoleRow,
) -> Result<HostPolicyRole, AppError> {
    let role_id = row.id;

    let atom_names: Vec<String> = sql_query(
        "SELECT a.name FROM host_policy_role_atoms ra
         JOIN host_policy_atoms a ON a.id = ra.atom_id
         WHERE ra.role_id = $1 ORDER BY a.name",
    )
    .bind::<SqlUuid, _>(role_id)
    .load::<NameRow>(c)?
    .into_iter()
    .map(|r| r.name)
    .collect();

    let host_names: Vec<String> = sql_query(
        "SELECT h.name FROM host_policy_role_hosts rh
         JOIN hosts h ON h.id = rh.host_id
         WHERE rh.role_id = $1 ORDER BY h.name",
    )
    .bind::<SqlUuid, _>(role_id)
    .load::<NameRow>(c)?
    .into_iter()
    .map(|r| r.name)
    .collect();

    let label_names: Vec<String> = sql_query(
        "SELECT l.name FROM host_policy_role_labels rl
         JOIN labels l ON l.id = rl.label_id
         WHERE rl.role_id = $1 ORDER BY l.name",
    )
    .bind::<SqlUuid, _>(role_id)
    .load::<NameRow>(c)?
    .into_iter()
    .map(|r| r.name)
    .collect();

    Ok(HostPolicyRole::restore(
        row.id,
        HostPolicyName::new(row.name)?,
        row.description,
        atom_names,
        host_names,
        label_names,
        row.created_at,
        row.updated_at,
    ))
}

fn build_roles_from_rows(
    c: &mut diesel::PgConnection,
    rows: Vec<RoleRow>,
) -> Result<Vec<HostPolicyRole>, AppError> {
    use crate::db::schema::{
        host_policy_atoms, host_policy_role_atoms, host_policy_role_hosts, host_policy_role_labels,
        hosts, labels,
    };

    let role_ids = rows.iter().map(|row| row.id).collect::<Vec<_>>();
    if role_ids.is_empty() {
        return Ok(Vec::new());
    }

    let atom_pairs = host_policy_role_atoms::table
        .inner_join(host_policy_atoms::table)
        .filter(host_policy_role_atoms::role_id.eq_any(&role_ids))
        .select((host_policy_role_atoms::role_id, host_policy_atoms::name))
        .order((
            host_policy_role_atoms::role_id.asc(),
            host_policy_atoms::name.asc(),
        ))
        .load::<(Uuid, String)>(c)?;

    let mut atom_map: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (role_id, name) in atom_pairs {
        atom_map.entry(role_id).or_default().push(name);
    }

    let host_pairs = host_policy_role_hosts::table
        .inner_join(hosts::table)
        .filter(host_policy_role_hosts::role_id.eq_any(&role_ids))
        .select((host_policy_role_hosts::role_id, hosts::name))
        .order((host_policy_role_hosts::role_id.asc(), hosts::name.asc()))
        .load::<(Uuid, String)>(c)?;

    let mut host_map: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (role_id, name) in host_pairs {
        host_map.entry(role_id).or_default().push(name);
    }

    let label_pairs = host_policy_role_labels::table
        .inner_join(labels::table)
        .filter(host_policy_role_labels::role_id.eq_any(&role_ids))
        .select((host_policy_role_labels::role_id, labels::name))
        .order((host_policy_role_labels::role_id.asc(), labels::name.asc()))
        .load::<(Uuid, String)>(c)?;

    let mut label_map: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (role_id, name) in label_pairs {
        label_map.entry(role_id).or_default().push(name);
    }

    rows.into_iter()
        .map(|row| {
            Ok(HostPolicyRole::restore(
                row.id,
                HostPolicyName::new(row.name)?,
                row.description,
                atom_map.remove(&row.id).unwrap_or_default(),
                host_map.remove(&row.id).unwrap_or_default(),
                label_map.remove(&row.id).unwrap_or_default(),
                row.created_at,
                row.updated_at,
            ))
        })
        .collect()
}
