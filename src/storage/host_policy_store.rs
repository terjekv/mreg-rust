use async_trait::async_trait;

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
};

/// CRUD operations for host-policy atoms and roles, including membership management.
#[async_trait]
pub trait HostPolicyStore: Send + Sync {
    async fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError>;
    async fn create_atom(&self, command: CreateHostPolicyAtom) -> Result<HostPolicyAtom, AppError>;
    async fn get_atom_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError>;
    async fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError>;
    async fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError>;

    async fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError>;
    async fn list_roles_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostPolicyRole>, AppError>;
    async fn create_role(&self, command: CreateHostPolicyRole) -> Result<HostPolicyRole, AppError>;
    async fn get_role_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError>;
    async fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError>;
    async fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError>;

    async fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError>;
    async fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError>;
    async fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError>;
    async fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError>;
    async fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError>;
    async fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError>;
}
