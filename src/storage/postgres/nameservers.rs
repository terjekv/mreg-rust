use async_trait::async_trait;
use diesel::{
    ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    delete, insert_into, update,
};

use crate::{
    db::{models::NameServerRow, schema::nameservers},
    domain::{
        nameserver::{CreateNameServer, NameServer, UpdateNameServer},
        pagination::{Page, PageRequest},
        types::DnsName,
    },
    errors::AppError,
    storage::NameServerStore,
};

use super::PostgresStorage;
use super::helpers::{map_unique, vec_to_page};

impl PostgresStorage {
    pub(super) fn query_nameservers(
        connection: &mut PgConnection,
    ) -> Result<Vec<NameServer>, AppError> {
        let rows = nameservers::table
            .order(nameservers::name.asc())
            .load::<NameServerRow>(connection)?;
        rows.into_iter().map(NameServerRow::into_domain).collect()
    }

    pub(super) fn list_nameservers_in_conn(
        connection: &mut PgConnection,
        page: &PageRequest,
    ) -> Result<Page<NameServer>, AppError> {
        let items = Self::query_nameservers(connection)?;
        Ok(vec_to_page(items, page))
    }

    pub(super) fn create_nameserver_in_conn(
        connection: &mut PgConnection,
        command: CreateNameServer,
    ) -> Result<NameServer, AppError> {
        let name = command.name().as_str().to_string();
        let ttl = command.ttl().map(|value| value.as_i32());
        insert_into(nameservers::table)
            .values((nameservers::name.eq(&name), nameservers::ttl.eq(ttl)))
            .returning(NameServerRow::as_returning())
            .get_result(connection)
            .map_err(map_unique("nameserver already exists"))?
            .into_domain()
    }

    pub(super) fn get_nameserver_by_name_in_conn(
        connection: &mut PgConnection,
        name: &DnsName,
    ) -> Result<NameServer, AppError> {
        let name = name.as_str().to_string();
        nameservers::table
            .filter(nameservers::name.eq(&name))
            .first::<NameServerRow>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found(format!("nameserver '{}' was not found", name)))?
            .into_domain()
    }

    pub(super) fn update_nameserver_in_conn(
        connection: &mut PgConnection,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        let name = name.as_str().to_string();
        if command.ttl.is_changed() {
            let ttl = command.ttl.into_set().map(|t| t.as_i32());
            update(nameservers::table.filter(nameservers::name.eq(&name)))
                .set((
                    nameservers::ttl.eq(ttl),
                    nameservers::updated_at.eq(diesel::dsl::now),
                ))
                .returning(NameServerRow::as_returning())
                .get_result::<NameServerRow>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("nameserver '{}' was not found", name))
                })?
                .into_domain()
        } else {
            nameservers::table
                .filter(nameservers::name.eq(&name))
                .first::<NameServerRow>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found(format!("nameserver '{}' was not found", name))
                })?
                .into_domain()
        }
    }

    pub(super) fn delete_nameserver_in_conn(
        connection: &mut PgConnection,
        name: &DnsName,
    ) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        let deleted = delete(nameservers::table.filter(nameservers::name.eq(&name)))
            .execute(connection)
            .map_err(|error| match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                    _,
                ) => AppError::conflict("nameserver is still referenced by another resource"),
                other => AppError::internal(other),
            })?;
        if deleted == 0 {
            return Err(AppError::not_found(format!(
                "nameserver '{}' was not found",
                name
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl NameServerStore for PostgresStorage {
    async fn list_nameservers(&self, page: &PageRequest) -> Result<Page<NameServer>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| Self::list_nameservers_in_conn(c, &page))
            .await
    }

    async fn create_nameserver(&self, command: CreateNameServer) -> Result<NameServer, AppError> {
        self.database
            .run(move |c| Self::create_nameserver_in_conn(c, command))
            .await
    }

    async fn get_nameserver_by_name(&self, name: &DnsName) -> Result<NameServer, AppError> {
        let name = name.clone();
        self.database
            .run(move |c| Self::get_nameserver_by_name_in_conn(c, &name))
            .await
    }

    async fn update_nameserver(
        &self,
        name: &DnsName,
        command: UpdateNameServer,
    ) -> Result<NameServer, AppError> {
        let name = name.clone();
        self.database
            .run(move |c| Self::update_nameserver_in_conn(c, &name, command))
            .await
    }

    async fn delete_nameserver(&self, name: &DnsName) -> Result<(), AppError> {
        let name = name.clone();
        self.database
            .run(move |c| Self::delete_nameserver_in_conn(c, &name))
            .await
    }
}
