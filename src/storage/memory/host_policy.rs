use async_trait::async_trait;
use chrono::Utc;
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

use super::{MemoryState, MemoryStorage, sort_and_paginate};

pub(super) fn list_atoms_in_state(
    state: &MemoryState,
    page: &PageRequest,
) -> Result<Page<HostPolicyAtom>, AppError> {
    let items: Vec<HostPolicyAtom> = state.host_policy_atoms.values().cloned().collect();
    sort_and_paginate(
        items,
        page,
        &["description", "created_at"],
        |atom, field| match field {
            "description" => atom.description().to_string(),
            "created_at" => atom.created_at().to_rfc3339(),
            _ => atom.name().as_str().to_string(),
        },
    )
}

pub(super) fn create_atom_in_state(
    state: &mut MemoryState,
    command: CreateHostPolicyAtom,
) -> Result<HostPolicyAtom, AppError> {
    let key = command.name().as_str().to_string();
    if state.host_policy_atoms.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "host policy atom '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let atom = HostPolicyAtom::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description().to_string(),
        now,
        now,
    );
    state.host_policy_atoms.insert(key, atom.clone());
    Ok(atom)
}

pub(super) fn get_atom_by_name_in_state(
    state: &MemoryState,
    name: &HostPolicyName,
) -> Result<HostPolicyAtom, AppError> {
    state
        .host_policy_atoms
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy atom '{}' was not found",
                name.as_str()
            ))
        })
}

pub(super) fn update_atom_in_state(
    state: &mut MemoryState,
    name: &HostPolicyName,
    command: UpdateHostPolicyAtom,
) -> Result<HostPolicyAtom, AppError> {
    let atom = state
        .host_policy_atoms
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy atom '{}' was not found",
                name.as_str()
            ))
        })?;
    let now = Utc::now();
    let description = command
        .description
        .unwrap_or_else(|| atom.description().to_string());
    let updated = HostPolicyAtom::restore(
        atom.id(),
        atom.name().clone(),
        description,
        atom.created_at(),
        now,
    );
    state
        .host_policy_atoms
        .insert(name.as_str().to_string(), updated.clone());
    Ok(updated)
}

pub(super) fn delete_atom_in_state(
    state: &mut MemoryState,
    name: &HostPolicyName,
) -> Result<(), AppError> {
    // Check if any role references this atom (RESTRICT behavior)
    for role in state.host_policy_roles.values() {
        if role.atoms().iter().any(|a| a == name.as_str()) {
            return Err(AppError::conflict(format!(
                "host policy atom '{}' is in use by role '{}'",
                name.as_str(),
                role.name().as_str()
            )));
        }
    }
    match state.host_policy_atoms.remove(name.as_str()) {
        Some(_removed) => Ok(()),
        None => Err(AppError::not_found(format!(
            "host policy atom '{}' was not found",
            name.as_str()
        ))),
    }
}

pub(super) fn list_roles_in_state(
    state: &MemoryState,
    page: &PageRequest,
) -> Result<Page<HostPolicyRole>, AppError> {
    let items: Vec<HostPolicyRole> = state.host_policy_roles.values().cloned().collect();
    sort_and_paginate(
        items,
        page,
        &["description", "created_at"],
        |role, field| match field {
            "description" => role.description().to_string(),
            "created_at" => role.created_at().to_rfc3339(),
            _ => role.name().as_str().to_string(),
        },
    )
}

pub(super) fn list_roles_for_host_in_state(
    state: &MemoryState,
    host_name: &Hostname,
) -> Result<Vec<HostPolicyRole>, AppError> {
    let mut items: Vec<HostPolicyRole> = state
        .host_policy_roles
        .values()
        .filter(|role| role.hosts().iter().any(|value| value == host_name.as_str()))
        .cloned()
        .collect();
    items.sort_by(|left, right| left.name().as_str().cmp(right.name().as_str()));
    Ok(items)
}

pub(super) fn list_roles_for_hosts_in_state(
    state: &MemoryState,
    hosts: &[Hostname],
) -> Result<Vec<HostPolicyRole>, AppError> {
    let host_names = hosts
        .iter()
        .map(|host| host.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut items: Vec<HostPolicyRole> = state
        .host_policy_roles
        .values()
        .filter(|role| {
            role.hosts()
                .iter()
                .any(|value| host_names.contains(value.as_str()))
        })
        .cloned()
        .collect();
    items.sort_by(|left, right| left.name().as_str().cmp(right.name().as_str()));
    Ok(items)
}

pub(super) fn create_role_in_state(
    state: &mut MemoryState,
    command: CreateHostPolicyRole,
) -> Result<HostPolicyRole, AppError> {
    let key = command.name().as_str().to_string();
    if state.host_policy_roles.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "host policy role '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let role = HostPolicyRole::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.description().to_string(),
        vec![],
        vec![],
        vec![],
        now,
        now,
    );
    state.host_policy_roles.insert(key, role.clone());
    Ok(role)
}

pub(super) fn get_role_by_name_in_state(
    state: &MemoryState,
    name: &HostPolicyName,
) -> Result<HostPolicyRole, AppError> {
    state
        .host_policy_roles
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                name.as_str()
            ))
        })
}

pub(super) fn update_role_in_state(
    state: &mut MemoryState,
    name: &HostPolicyName,
    command: UpdateHostPolicyRole,
) -> Result<HostPolicyRole, AppError> {
    let role = state
        .host_policy_roles
        .get(name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                name.as_str()
            ))
        })?;
    let now = Utc::now();
    let description = command
        .description
        .unwrap_or_else(|| role.description().to_string());
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        description,
        role.atoms().to_vec(),
        role.hosts().to_vec(),
        role.labels().to_vec(),
        role.created_at(),
        now,
    );
    state
        .host_policy_roles
        .insert(name.as_str().to_string(), updated.clone());
    Ok(updated)
}

pub(super) fn delete_role_in_state(
    state: &mut MemoryState,
    name: &HostPolicyName,
) -> Result<(), AppError> {
    match state.host_policy_roles.remove(name.as_str()) {
        Some(_removed) => Ok(()),
        None => Err(AppError::not_found(format!(
            "host policy role '{}' was not found",
            name.as_str()
        ))),
    }
}

pub(super) fn add_atom_to_role_in_state(
    state: &mut MemoryState,
    role_name: &HostPolicyName,
    atom_name: &HostPolicyName,
) -> Result<(), AppError> {
    // Verify atom exists
    if !state.host_policy_atoms.contains_key(atom_name.as_str()) {
        return Err(AppError::not_found(format!(
            "host policy atom '{}' was not found",
            atom_name.as_str()
        )));
    }
    let role = state
        .host_policy_roles
        .get(role_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                role_name.as_str()
            ))
        })?;
    let atom_str = atom_name.as_str().to_string();
    if role.atoms().contains(&atom_str) {
        return Err(AppError::conflict(format!(
            "atom '{}' is already in role '{}'",
            atom_name.as_str(),
            role_name.as_str()
        )));
    }
    let mut atoms = role.atoms().to_vec();
    atoms.push(atom_str);
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        role.description().to_string(),
        atoms,
        role.hosts().to_vec(),
        role.labels().to_vec(),
        role.created_at(),
        Utc::now(),
    );
    state
        .host_policy_roles
        .insert(role_name.as_str().to_string(), updated);
    Ok(())
}

pub(super) fn remove_atom_from_role_in_state(
    state: &mut MemoryState,
    role_name: &HostPolicyName,
    atom_name: &HostPolicyName,
) -> Result<(), AppError> {
    let role = state
        .host_policy_roles
        .get(role_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                role_name.as_str()
            ))
        })?;
    let atom_str = atom_name.as_str().to_string();
    if !role.atoms().contains(&atom_str) {
        return Err(AppError::not_found(format!(
            "atom '{}' is not in role '{}'",
            atom_name.as_str(),
            role_name.as_str()
        )));
    }
    let atoms: Vec<String> = role
        .atoms()
        .iter()
        .filter(|a| a.as_str() != atom_name.as_str())
        .cloned()
        .collect();
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        role.description().to_string(),
        atoms,
        role.hosts().to_vec(),
        role.labels().to_vec(),
        role.created_at(),
        Utc::now(),
    );
    state
        .host_policy_roles
        .insert(role_name.as_str().to_string(), updated);
    Ok(())
}

pub(super) fn add_host_to_role_in_state(
    state: &mut MemoryState,
    role_name: &HostPolicyName,
    host_name: &str,
) -> Result<(), AppError> {
    // Verify host exists
    if !state.hosts.contains_key(host_name) {
        return Err(AppError::not_found(format!(
            "host '{}' was not found",
            host_name
        )));
    }
    let role = state
        .host_policy_roles
        .get(role_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                role_name.as_str()
            ))
        })?;
    let host_str = host_name.to_string();
    if role.hosts().contains(&host_str) {
        return Err(AppError::conflict(format!(
            "host '{}' is already in role '{}'",
            host_name,
            role_name.as_str()
        )));
    }
    let mut hosts = role.hosts().to_vec();
    hosts.push(host_str);
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        role.description().to_string(),
        role.atoms().to_vec(),
        hosts,
        role.labels().to_vec(),
        role.created_at(),
        Utc::now(),
    );
    state
        .host_policy_roles
        .insert(role_name.as_str().to_string(), updated);
    Ok(())
}

pub(super) fn remove_host_from_role_in_state(
    state: &mut MemoryState,
    role_name: &HostPolicyName,
    host_name: &str,
) -> Result<(), AppError> {
    let role = state
        .host_policy_roles
        .get(role_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                role_name.as_str()
            ))
        })?;
    let host_str = host_name.to_string();
    if !role.hosts().contains(&host_str) {
        return Err(AppError::not_found(format!(
            "host '{}' is not in role '{}'",
            host_name,
            role_name.as_str()
        )));
    }
    let hosts: Vec<String> = role
        .hosts()
        .iter()
        .filter(|h| h.as_str() != host_name)
        .cloned()
        .collect();
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        role.description().to_string(),
        role.atoms().to_vec(),
        hosts,
        role.labels().to_vec(),
        role.created_at(),
        Utc::now(),
    );
    state
        .host_policy_roles
        .insert(role_name.as_str().to_string(), updated);
    Ok(())
}

pub(super) fn add_label_to_role_in_state(
    state: &mut MemoryState,
    role_name: &HostPolicyName,
    label_name: &str,
) -> Result<(), AppError> {
    // Verify label exists
    if !state.labels.contains_key(label_name) {
        return Err(AppError::not_found(format!(
            "label '{}' was not found",
            label_name
        )));
    }
    let role = state
        .host_policy_roles
        .get(role_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                role_name.as_str()
            ))
        })?;
    let label_str = label_name.to_string();
    if role.labels().contains(&label_str) {
        return Err(AppError::conflict(format!(
            "label '{}' is already in role '{}'",
            label_name,
            role_name.as_str()
        )));
    }
    let mut labels = role.labels().to_vec();
    labels.push(label_str);
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        role.description().to_string(),
        role.atoms().to_vec(),
        role.hosts().to_vec(),
        labels,
        role.created_at(),
        Utc::now(),
    );
    state
        .host_policy_roles
        .insert(role_name.as_str().to_string(), updated);
    Ok(())
}

pub(super) fn remove_label_from_role_in_state(
    state: &mut MemoryState,
    role_name: &HostPolicyName,
    label_name: &str,
) -> Result<(), AppError> {
    let role = state
        .host_policy_roles
        .get(role_name.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host policy role '{}' was not found",
                role_name.as_str()
            ))
        })?;
    let label_str = label_name.to_string();
    if !role.labels().contains(&label_str) {
        return Err(AppError::not_found(format!(
            "label '{}' is not in role '{}'",
            label_name,
            role_name.as_str()
        )));
    }
    let labels: Vec<String> = role
        .labels()
        .iter()
        .filter(|l| l.as_str() != label_name)
        .cloned()
        .collect();
    let updated = HostPolicyRole::restore(
        role.id(),
        role.name().clone(),
        role.description().to_string(),
        role.atoms().to_vec(),
        role.hosts().to_vec(),
        labels,
        role.created_at(),
        Utc::now(),
    );
    state
        .host_policy_roles
        .insert(role_name.as_str().to_string(), updated);
    Ok(())
}

#[async_trait]
impl HostPolicyStore for MemoryStorage {
    async fn list_atoms(&self, page: &PageRequest) -> Result<Page<HostPolicyAtom>, AppError> {
        let state = self.state.read().await;
        list_atoms_in_state(&state, page)
    }

    async fn create_atom(&self, command: CreateHostPolicyAtom) -> Result<HostPolicyAtom, AppError> {
        let mut state = self.state.write().await;
        create_atom_in_state(&mut state, command)
    }

    async fn get_atom_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyAtom, AppError> {
        let state = self.state.read().await;
        get_atom_by_name_in_state(&state, name)
    }

    async fn update_atom(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyAtom,
    ) -> Result<HostPolicyAtom, AppError> {
        let mut state = self.state.write().await;
        update_atom_in_state(&mut state, name, command)
    }

    async fn delete_atom(&self, name: &HostPolicyName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_atom_in_state(&mut state, name)
    }

    async fn list_roles(&self, page: &PageRequest) -> Result<Page<HostPolicyRole>, AppError> {
        let state = self.state.read().await;
        list_roles_in_state(&state, page)
    }

    async fn list_roles_for_host(
        &self,
        host_name: &Hostname,
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        let state = self.state.read().await;
        list_roles_for_host_in_state(&state, host_name)
    }

    async fn list_roles_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostPolicyRole>, AppError> {
        let state = self.state.read().await;
        list_roles_for_hosts_in_state(&state, hosts)
    }

    async fn create_role(&self, command: CreateHostPolicyRole) -> Result<HostPolicyRole, AppError> {
        let mut state = self.state.write().await;
        create_role_in_state(&mut state, command)
    }

    async fn get_role_by_name(&self, name: &HostPolicyName) -> Result<HostPolicyRole, AppError> {
        let state = self.state.read().await;
        get_role_by_name_in_state(&state, name)
    }

    async fn update_role(
        &self,
        name: &HostPolicyName,
        command: UpdateHostPolicyRole,
    ) -> Result<HostPolicyRole, AppError> {
        let mut state = self.state.write().await;
        update_role_in_state(&mut state, name, command)
    }

    async fn delete_role(&self, name: &HostPolicyName) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        delete_role_in_state(&mut state, name)
    }

    async fn add_atom_to_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        add_atom_to_role_in_state(&mut state, role_name, atom_name)
    }

    async fn remove_atom_from_role(
        &self,
        role_name: &HostPolicyName,
        atom_name: &HostPolicyName,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        remove_atom_from_role_in_state(&mut state, role_name, atom_name)
    }

    async fn add_host_to_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        add_host_to_role_in_state(&mut state, role_name, host_name)
    }

    async fn remove_host_from_role(
        &self,
        role_name: &HostPolicyName,
        host_name: &str,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        remove_host_from_role_in_state(&mut state, role_name, host_name)
    }

    async fn add_label_to_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        add_label_to_role_in_state(&mut state, role_name, label_name)
    }

    async fn remove_label_from_role(
        &self,
        role_name: &HostPolicyName,
        label_name: &str,
    ) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        remove_label_from_role_in_state(&mut state, role_name, label_name)
    }
}
