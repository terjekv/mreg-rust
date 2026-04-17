use async_trait::async_trait;

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
};

/// CRUD operations for forward and reverse DNS zones, including delegations and serial bumps.
#[async_trait]
pub trait ZoneStore: Send + Sync {
    async fn list_forward_zones(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError>;
    async fn create_forward_zone(
        &self,
        command: CreateForwardZone,
    ) -> Result<ForwardZone, AppError>;
    async fn get_forward_zone_by_name(&self, name: &ZoneName) -> Result<ForwardZone, AppError>;
    async fn update_forward_zone(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError>;
    async fn delete_forward_zone(&self, name: &ZoneName) -> Result<(), AppError>;

    async fn list_forward_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError>;
    async fn create_forward_zone_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError>;
    async fn delete_forward_zone_delegation(
        &self,
        delegation_id: uuid::Uuid,
    ) -> Result<(), AppError>;

    async fn bump_forward_zone_serial(&self, zone_id: uuid::Uuid) -> Result<ForwardZone, AppError>;

    async fn list_reverse_zones(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError>;
    async fn create_reverse_zone(
        &self,
        command: CreateReverseZone,
    ) -> Result<ReverseZone, AppError>;
    async fn get_reverse_zone_by_name(&self, name: &ZoneName) -> Result<ReverseZone, AppError>;
    async fn update_reverse_zone(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError>;
    async fn delete_reverse_zone(&self, name: &ZoneName) -> Result<(), AppError>;

    async fn list_reverse_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError>;
    async fn create_reverse_zone_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError>;
    async fn delete_reverse_zone_delegation(
        &self,
        delegation_id: uuid::Uuid,
    ) -> Result<(), AppError>;

    async fn bump_reverse_zone_serial(&self, zone_id: uuid::Uuid) -> Result<ReverseZone, AppError>;
}
