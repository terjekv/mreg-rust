mod delegations;
mod forward;
mod reverse;

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    domain::{
        pagination::{Page, PageRequest},
        types::ZoneName,
        zone::{
            CreateForwardZone, CreateForwardZoneDelegation, CreateReverseZone,
            CreateReverseZoneDelegation, ForwardZone, ForwardZoneDelegation, ReverseZone,
            ReverseZoneDelegation, UpdateForwardZone, UpdateReverseZone,
        },
    },
    errors::AppError,
    storage::ZoneStore,
};

use super::PostgresStorage;

#[async_trait]
impl ZoneStore for PostgresStorage {
    async fn list_forward_zones(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| Self::list_forward_zones_impl(c, &page))
            .await
    }

    async fn create_forward_zone(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        self.database
            .run(move |c| Self::create_forward_zone_impl(c, command))
            .await
    }

    async fn get_forward_zone_by_name(&self, name: &ZoneName) -> Result<ForwardZone, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| Self::get_forward_zone_by_name_impl(c, &name))
            .await
    }

    async fn update_forward_zone(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| Self::update_forward_zone_impl(c, &name, command))
            .await
    }

    async fn delete_forward_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| Self::delete_forward_zone_impl(c, &name))
            .await
    }

    async fn list_forward_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError> {
        let name = zone_name.as_str().to_string();
        let page = page.clone();
        self.database
            .run(move |c| Self::list_forward_zone_delegations_impl(c, &name, &page))
            .await
    }

    async fn create_forward_zone_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError> {
        self.database
            .run(move |c| Self::create_forward_zone_delegation_impl(c, command))
            .await
    }

    async fn delete_forward_zone_delegation(
        &self,
        delegation_id: uuid::Uuid,
    ) -> Result<(), AppError> {
        self.database
            .run(move |c| Self::delete_forward_zone_delegation_impl(c, delegation_id))
            .await
    }

    async fn bump_forward_zone_serial(&self, zone_id: Uuid) -> Result<ForwardZone, AppError> {
        self.database
            .run(move |c| Self::bump_forward_zone_serial_impl(c, zone_id))
            .await
    }

    async fn list_reverse_zones(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| Self::list_reverse_zones_impl(c, &page))
            .await
    }

    async fn create_reverse_zone(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        self.database
            .run(move |c| Self::create_reverse_zone_impl(c, command))
            .await
    }

    async fn get_reverse_zone_by_name(&self, name: &ZoneName) -> Result<ReverseZone, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| Self::get_reverse_zone_by_name_impl(c, &name))
            .await
    }

    async fn update_reverse_zone(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| Self::update_reverse_zone_impl(c, &name, command))
            .await
    }

    async fn delete_reverse_zone(&self, name: &ZoneName) -> Result<(), AppError> {
        let name = name.as_str().to_string();
        self.database
            .run(move |c| Self::delete_reverse_zone_impl(c, &name))
            .await
    }

    async fn list_reverse_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError> {
        let name = zone_name.as_str().to_string();
        let page = page.clone();
        self.database
            .run(move |c| Self::list_reverse_zone_delegations_impl(c, &name, &page))
            .await
    }

    async fn create_reverse_zone_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError> {
        self.database
            .run(move |c| Self::create_reverse_zone_delegation_impl(c, command))
            .await
    }

    async fn delete_reverse_zone_delegation(
        &self,
        delegation_id: uuid::Uuid,
    ) -> Result<(), AppError> {
        self.database
            .run(move |c| Self::delete_reverse_zone_delegation_impl(c, delegation_id))
            .await
    }

    async fn bump_reverse_zone_serial(&self, zone_id: Uuid) -> Result<ReverseZone, AppError> {
        self.database
            .run(move |c| Self::bump_reverse_zone_serial_impl(c, zone_id))
            .await
    }
}
