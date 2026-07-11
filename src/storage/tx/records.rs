use crate::{
    domain::{
        filters::RecordFilter,
        pagination::{Page, PageRequest},
        resource_records::{
            CreateRecordInstance, CreateRecordTypeDefinition, RecordInstance, RecordRrset,
            RecordTypeDefinition, UpdateRecord,
        },
        types::{DnsName, Hostname, RecordTypeName},
    },
    errors::AppError,
};

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::RecordStore`].
pub trait TxRecordStore {
    fn list_record_types(
        &self,
        page: &PageRequest,
    ) -> Result<Page<RecordTypeDefinition>, AppError>;
    fn list_rrsets(&self, page: &PageRequest) -> Result<Page<RecordRrset>, AppError>;
    fn list_records(
        &self,
        page: &PageRequest,
        filter: &RecordFilter,
    ) -> Result<Page<RecordInstance>, AppError>;
    fn get_record(&self, record_id: uuid::Uuid) -> Result<RecordInstance, AppError>;
    fn get_rrset(&self, rrset_id: uuid::Uuid) -> Result<RecordRrset, AppError>;
    fn list_records_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<RecordInstance>, AppError>;
    fn create_record_type(
        &self,
        command: CreateRecordTypeDefinition,
    ) -> Result<RecordTypeDefinition, AppError>;
    fn create_record(
        &self,
        command: CreateRecordInstance,
    ) -> Result<RecordInstance, AppError>;
    fn update_record(
        &self,
        record_id: uuid::Uuid,
        command: UpdateRecord,
    ) -> Result<RecordInstance, AppError>;
    fn delete_record(&self, record_id: uuid::Uuid) -> Result<(), AppError>;
    fn delete_record_type(&self, name: &RecordTypeName) -> Result<(), AppError>;
    fn delete_rrset(&self, rrset_id: uuid::Uuid) -> Result<(), AppError>;
    fn find_records_by_owner(
        &self,
        owner_id: uuid::Uuid,
    ) -> Result<Vec<RecordInstance>, AppError>;
    fn delete_records_by_owner(&self, owner_id: uuid::Uuid) -> Result<u64, AppError>;
    fn delete_records_by_owner_name_and_type(
        &self,
        owner_name: &DnsName,
        type_name: &RecordTypeName,
    ) -> Result<u64, AppError>;
    fn rename_record_owner(
        &self,
        owner_id: uuid::Uuid,
        new_name: &DnsName,
    ) -> Result<u64, AppError>;
}
