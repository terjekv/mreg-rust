use async_trait::async_trait;
use diesel::{
    ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
    delete, insert_into, update,
};

use crate::{
    db::{models::LabelRow, schema::labels},
    domain::{
        label::{CreateLabel, Label, UpdateLabel},
        pagination::{Page, PageRequest, SortDirection},
        types::LabelName,
    },
    errors::AppError,
    storage::LabelStore,
};

use super::PostgresStorage;
use super::helpers::{map_unique, vec_to_page};

impl PostgresStorage {
    pub(super) fn query_labels(connection: &mut PgConnection) -> Result<Vec<Label>, AppError> {
        let rows = labels::table
            .order(labels::name.asc())
            .load::<LabelRow>(connection)?;
        rows.into_iter().map(LabelRow::into_domain).collect()
    }

    pub(super) fn list_labels_in_conn(
        connection: &mut PgConnection,
        page: &PageRequest,
    ) -> Result<Page<Label>, AppError> {
        let rows = match (page.sort_by(), page.sort_direction()) {
            (Some("description"), SortDirection::Desc) => labels::table
                .order(labels::description.desc())
                .load::<LabelRow>(connection)?,
            (Some("description"), _) => labels::table
                .order(labels::description.asc())
                .load::<LabelRow>(connection)?,
            (Some("created_at"), SortDirection::Desc) => labels::table
                .order(labels::created_at.desc())
                .load::<LabelRow>(connection)?,
            (Some("created_at"), _) => labels::table
                .order(labels::created_at.asc())
                .load::<LabelRow>(connection)?,
            (Some("name"), SortDirection::Desc) | (None, SortDirection::Desc) => labels::table
                .order(labels::name.desc())
                .load::<LabelRow>(connection)?,
            (Some("name"), _) | (None, _) => labels::table
                .order(labels::name.asc())
                .load::<LabelRow>(connection)?,
            (Some(other), _) => {
                return Err(AppError::validation(format!(
                    "unsupported sort_by field for labels: {other}"
                )));
            }
        };
        let items = rows
            .into_iter()
            .map(LabelRow::into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(vec_to_page(items, page))
    }

    pub(super) fn create_label_in_conn(
        connection: &mut PgConnection,
        command: CreateLabel,
    ) -> Result<Label, AppError> {
        let name = command.name().as_str().to_string();
        let description = command.description().to_string();
        insert_into(labels::table)
            .values((labels::name.eq(&name), labels::description.eq(&description)))
            .returning(LabelRow::as_returning())
            .get_result(connection)
            .map_err(map_unique("label already exists"))?
            .into_domain()
    }

    pub(super) fn get_label_by_name_in_conn(
        connection: &mut PgConnection,
        name: &LabelName,
    ) -> Result<Label, AppError> {
        let name = name.as_str().to_string();
        labels::table
            .filter(labels::name.eq(&name))
            .first::<LabelRow>(connection)
            .optional()?
            .ok_or_else(|| AppError::not_found(format!("label '{}' was not found", name)))?
            .into_domain()
    }

    pub(super) fn update_label_in_conn(
        connection: &mut PgConnection,
        name: &LabelName,
        command: UpdateLabel,
    ) -> Result<Label, AppError> {
        let name = name.as_str().to_string();
        if let Some(ref description) = command.description {
            update(labels::table.filter(labels::name.eq(&name)))
                .set((
                    labels::description.eq(description),
                    labels::updated_at.eq(diesel::dsl::now),
                ))
                .returning(LabelRow::as_returning())
                .get_result::<LabelRow>(connection)
                .optional()?
                .ok_or_else(|| AppError::not_found(format!("label '{}' was not found", name)))?
                .into_domain()
        } else {
            labels::table
                .filter(labels::name.eq(&name))
                .first::<LabelRow>(connection)
                .optional()?
                .ok_or_else(|| AppError::not_found(format!("label '{}' was not found", name)))?
                .into_domain()
        }
    }

    pub(super) fn delete_label_in_conn(
        connection: &mut PgConnection,
        name: &LabelName,
    ) -> Result<(), AppError> {
        let name_str = name.as_str().to_string();
        let deleted =
            delete(labels::table.filter(labels::name.eq(&name_str))).execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found(format!(
                "label '{}' was not found",
                name_str
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl LabelStore for PostgresStorage {
    async fn list_labels(&self, page: &PageRequest) -> Result<Page<Label>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| Self::list_labels_in_conn(c, &page))
            .await
    }

    async fn create_label(&self, command: CreateLabel) -> Result<Label, AppError> {
        self.database
            .run(move |c| Self::create_label_in_conn(c, command))
            .await
    }

    async fn get_label_by_name(&self, name: &LabelName) -> Result<Label, AppError> {
        let name = name.clone();
        self.database
            .run(move |c| Self::get_label_by_name_in_conn(c, &name))
            .await
    }

    async fn update_label(
        &self,
        name: &LabelName,
        command: UpdateLabel,
    ) -> Result<Label, AppError> {
        let name = name.clone();
        self.database
            .run(move |c| Self::update_label_in_conn(c, &name, command))
            .await
    }

    async fn delete_label(&self, name: &LabelName) -> Result<(), AppError> {
        let name = name.clone();
        self.database
            .run(move |c| Self::delete_label_in_conn(c, &name))
            .await
    }
}
