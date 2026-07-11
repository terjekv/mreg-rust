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

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::ZoneStore`].
pub trait TxZoneStore {
    fn list_forward_zones(&self, page: &PageRequest) -> Result<Page<ForwardZone>, AppError>;
    fn create_forward_zone(&self, command: CreateForwardZone) -> Result<ForwardZone, AppError>;
    fn get_forward_zone_by_name(&self, name: &ZoneName) -> Result<ForwardZone, AppError>;
    fn update_forward_zone(
        &self,
        name: &ZoneName,
        command: UpdateForwardZone,
    ) -> Result<ForwardZone, AppError>;
    fn delete_forward_zone(&self, name: &ZoneName) -> Result<(), AppError>;

    fn list_forward_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ForwardZoneDelegation>, AppError>;
    fn create_forward_zone_delegation(
        &self,
        command: CreateForwardZoneDelegation,
    ) -> Result<ForwardZoneDelegation, AppError>;
    fn delete_forward_zone_delegation(&self, delegation_id: uuid::Uuid) -> Result<(), AppError>;

    fn bump_forward_zone_serial(&self, zone_id: uuid::Uuid) -> Result<ForwardZone, AppError>;

    fn list_reverse_zones(&self, page: &PageRequest) -> Result<Page<ReverseZone>, AppError>;
    fn create_reverse_zone(&self, command: CreateReverseZone) -> Result<ReverseZone, AppError>;
    fn get_reverse_zone_by_name(&self, name: &ZoneName) -> Result<ReverseZone, AppError>;
    fn update_reverse_zone(
        &self,
        name: &ZoneName,
        command: UpdateReverseZone,
    ) -> Result<ReverseZone, AppError>;
    fn delete_reverse_zone(&self, name: &ZoneName) -> Result<(), AppError>;

    fn list_reverse_zone_delegations(
        &self,
        zone_name: &ZoneName,
        page: &PageRequest,
    ) -> Result<Page<ReverseZoneDelegation>, AppError>;
    fn create_reverse_zone_delegation(
        &self,
        command: CreateReverseZoneDelegation,
    ) -> Result<ReverseZoneDelegation, AppError>;
    fn delete_reverse_zone_delegation(&self, delegation_id: uuid::Uuid) -> Result<(), AppError>;

    fn bump_reverse_zone_serial(&self, zone_id: uuid::Uuid) -> Result<ReverseZone, AppError>;
}
