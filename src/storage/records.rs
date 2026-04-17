use async_trait::async_trait;

use crate::{
    domain::{
        filters::RecordFilter,
        pagination::{Page, PageRequest},
        resource_records::{
            CreateRecordInstance, CreateRecordTypeDefinition, RecordInstance, RecordRrset,
            RecordTypeDefinition, UpdateRecord,
        },
        types::{DnsName, RecordTypeName},
    },
    errors::AppError,
};

/// CRUD operations for DNS record types, RRSets, and individual records.
#[async_trait]
pub trait RecordStore: Send + Sync {
    async fn list_record_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError>;
    async fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError>;
    async fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError>;
    async fn get_record(&self, record_id: uuid::Uuid) -> Result<RecordInstance, AppError>;
    async fn get_rrset(&self, rrset_id: uuid::Uuid) -> Result<RecordRrset, AppError>;
    async fn create_record_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError>;
    async fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError>;
    async fn update_record(
        &self,
        record_id: uuid::Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError>;
    async fn delete_record(&self, record_id: uuid::Uuid) -> Result<(), AppError>;
    async fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError>;
    async fn delete_rrset(&self, rrset_id: uuid::Uuid) -> Result<(), AppError>;
    async fn find_records_by_owner(
        &self,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<RecordInstance>, AppError>;
    async fn delete_records_by_owner(&self, owner_id: uuid::Uuid) -> Result<u64, AppError>;
    async fn delete_records_by_owner_name_and_type(
        &self,
        owner_name: &DnsName,
        type_name: &RecordTypeName,
    ) -> Result<u64, AppError>;
    async fn rename_record_owner(
        &self,
        owner_id: uuid::Uuid,
        new_name: &DnsName,
    ) -> Result<u64, AppError>;
}
