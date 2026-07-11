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

/// Synchronous, transaction-scoped 1:1 mirror of [`crate::storage::HostPolicyStore`].
pub trait TxHostPolicyStore {
    fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError>;
    fn create_atom(&self, command: CreateHostPolicyAtom) -> Result<HostPolicyAtom, AppError>;
    fn get_atom_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError>;
    fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError>;
    fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError>;

    fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError>;
    fn list_roles_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostPolicyRole>, AppError>;
    fn list_roles_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostPolicyRole>, AppError>;
    fn create_role(&self, command: CreateHostPolicyRole) -> Result<HostPolicyRole, AppError>;
    fn get_role_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError>;
    fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError>;
    fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError>;

    fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError>;
    fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError>;
    fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError>;
    fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError>;
    fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError>;
    fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError>;
}
