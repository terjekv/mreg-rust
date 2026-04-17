use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        filters::NetworkFilter,
        host::IpAddressAssignment,
        network::{
            CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork, ip_to_u128,
            network_usable_bounds,
        },
        pagination::{Page, PageRequest},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
    storage::NetworkStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

pub(super) fn create_network_in_state(
    state: &mut MemoryState,
    command: CreateNetwork,
) -> Result<Network, AppError> {
    let key = command.cidr().as_str();
    if state.networks.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "network '{}' already exists",
            key
        )));
    }
    let now = Utc::now();
    let network = Network::restore(
        Uuid::new_v4(),
        command.cidr().clone(),
        command.description().to_string(),
        command.vlan(),
        command.dns_delegated(),
        command.category().to_string(),
        command.location().to_string(),
        command.frozen(),
        command.reserved(),
        now,
        now,
    )?;
    state.networks.insert(key.clone(), network.clone());
    state.excluded_ranges.entry(key).or_default();
    Ok(network)
}

pub(super) fn add_excluded_range_in_state(
    state: &mut MemoryState,
    network: &CidrValue,
    command: CreateExcludedRange,
) -> Result<ExcludedRange, AppError> {
    let network_value = state
        .networks
        .get(&network.as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!("network '{}' was not found", network.as_str()))
        })?;
    if !network_value.contains(command.start_ip()) || !network_value.contains(command.end_ip()) {
        return Err(AppError::validation(
            "excluded range must be fully contained inside the network",
        ));
    }

    let entry = state.excluded_ranges.entry(network.as_str()).or_default();
    if entry.iter().any(|existing| {
        ip_to_u128(existing.start_ip().as_inner()) <= ip_to_u128(command.end_ip().as_inner())
            && ip_to_u128(command.start_ip().as_inner()) <= ip_to_u128(existing.end_ip().as_inner())
    }) {
        return Err(AppError::conflict(
            "excluded range overlaps an existing excluded range",
        ));
    }

    let now = Utc::now();
    let range = ExcludedRange::restore(
        Uuid::new_v4(),
        network_value.id(),
        *command.start_ip(),
        *command.end_ip(),
        command.description().to_string(),
        now,
        now,
    )?;
    entry.push(range.clone());
    Ok(range)
}

#[async_trait]
impl NetworkStore for MemoryStorage {
    async fn list_networks(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<Network> = state
            .networks
            .values()
            .filter(|network| filter.matches(network))
            .cloned()
            .collect();
        sort_items(
            &mut items,
            page,
            &["description", "created_at", "updated_at"],
            |network, field| match field {
                "description" => network.description().to_string(),
                "created_at" => network.created_at().to_rfc3339(),
                "updated_at" => network.updated_at().to_rfc3339(),
                _ => network.cidr().as_str(),
            },
        )?;
        paginate_by_cursor(items, page)
    }

    async fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError> {
        let mut state = self.state.write().await;
        let network = create_network_in_state(&mut state, command)?;
        Ok(network)
    }

    async fn get_network_by_cidr(&self, cidr: &CidrValue) -> Result<Network, AppError> {
        let state = self.state.read().await;
        state.networks.get(&cidr.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("network '{}' was not found", cidr.as_str()))
        })
    }

    async fn update_network(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        let mut state = self.state.write().await;
        let key = cidr.as_str();
        let network = state
            .networks
            .get(&key)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("network '{}' was not found", key)))?;
        let now = Utc::now();
        let description = command
            .description
            .unwrap_or_else(|| network.description().to_string());
        let vlan = command.vlan.resolve(network.vlan());
        let dns_delegated = command.dns_delegated.unwrap_or(network.dns_delegated());
        let category = command
            .category
            .unwrap_or_else(|| network.category().to_string());
        let location = command
            .location
            .unwrap_or_else(|| network.location().to_string());
        let frozen = command.frozen.unwrap_or(network.frozen());
        let reserved = command.reserved.unwrap_or(network.reserved());
        let updated = Network::restore(
            network.id(),
            network.cidr().clone(),
            description,
            vlan,
            dns_delegated,
            category,
            location,
            frozen,
            reserved,
            network.created_at(),
            now,
        )?;
        state.networks.insert(key.clone(), updated.clone());
        Ok(updated)
    }

    async fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let key = cidr.as_str();
        let network = state
            .networks
            .get(&key)
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("network '{}' was not found", key)))?;

        if state
            .ip_addresses
            .values()
            .any(|assignment| assignment.network_id() == network.id())
        {
            return Err(AppError::conflict(
                "network still has allocated IP addresses",
            ));
        }

        state.networks.remove(&key);
        state.excluded_ranges.remove(&key);
        Ok(())
    }

    async fn list_excluded_ranges(
        &self,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<ExcludedRange> = state
            .excluded_ranges
            .get(&network.as_str())
            .cloned()
            .unwrap_or_default();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn add_excluded_range(
        &self,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        let mut state = self.state.write().await;
        let range = add_excluded_range_in_state(&mut state, network, command)?;
        Ok(range)
    }

    async fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        let state = self.state.read().await;
        let network = state.networks.get(&cidr.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("network '{}' was not found", cidr.as_str()))
        })?;
        let mut assignments: Vec<IpAddressAssignment> = state
            .ip_addresses
            .values()
            .filter(|a| network.contains(a.address()))
            .cloned()
            .collect();
        assignments.sort_by_key(|a| ip_to_u128(a.address().as_inner()));
        Ok(assignments)
    }

    async fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        let state = self.state.read().await;
        let network = state.networks.get(&cidr.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("network '{}' was not found", cidr.as_str()))
        })?;
        let limit = limit.unwrap_or(100) as usize;
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        let excluded = state
            .excluded_ranges
            .get(&cidr.as_str())
            .cloned()
            .unwrap_or_default();
        let used: std::collections::HashSet<u128> = state
            .ip_addresses
            .values()
            .filter(|a| network.contains(a.address()))
            .map(|a| ip_to_u128(a.address().as_inner()))
            .collect();
        let mut result = Vec::new();
        match network.cidr().as_inner() {
            ipnet::IpNet::V4(_) => {
                for candidate in first..=last {
                    if result.len() >= limit {
                        break;
                    }
                    if used.contains(&candidate) {
                        continue;
                    }
                    let addr = IpAddressValue::new(
                        std::net::Ipv4Addr::from(candidate as u32).to_string(),
                    )?;
                    if excluded.iter().any(|r| r.contains(&addr)) {
                        continue;
                    }
                    result.push(addr);
                }
            }
            ipnet::IpNet::V6(_) => {
                for candidate in first..=last {
                    if result.len() >= limit {
                        break;
                    }
                    if used.contains(&candidate) {
                        continue;
                    }
                    let addr =
                        IpAddressValue::new(std::net::Ipv6Addr::from(candidate).to_string())?;
                    if excluded.iter().any(|r| r.contains(&addr)) {
                        continue;
                    }
                    result.push(addr);
                }
            }
        }
        Ok(result)
    }

    async fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError> {
        let state = self.state.read().await;
        let network = state.networks.get(&cidr.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("network '{}' was not found", cidr.as_str()))
        })?;
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        let excluded = state
            .excluded_ranges
            .get(&cidr.as_str())
            .cloned()
            .unwrap_or_default();
        let used: std::collections::HashSet<u128> = state
            .ip_addresses
            .values()
            .filter(|a| network.contains(a.address()))
            .map(|a| ip_to_u128(a.address().as_inner()))
            .collect();
        let usable_span = last.saturating_sub(first).saturating_add(1);
        let excluded_count = excluded
            .iter()
            .map(|range| {
                let start = ip_to_u128(range.start_ip().as_inner()).max(first);
                let end = ip_to_u128(range.end_ip().as_inner()).min(last);
                if start > end { 0 } else { end - start + 1 }
            })
            .sum::<u128>();
        Ok(usable_span
            .saturating_sub(used.len() as u128)
            .saturating_sub(excluded_count) as u64)
    }
}
