use std::collections::HashSet;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    domain::{
        attachment::{CreateAttachmentDhcpIdentifier, DhcpIdentifierFamily, DhcpIdentifierKind},
        filters::HostFilter,
        host::{
            AllocationPolicy, AssignIpAddress, CreateHost, Host, HostAuthContext,
            IpAddressAssignment, UpdateHost, UpdateIpAddress,
        },
        network::{Network, cidr_contains, ip_to_u128, network_usable_bounds},
        pagination::{Page, PageRequest},
        resource_records::{CreateRecordInstance, RecordInstance, RecordOwnerKind, RecordRrset},
        types::ip_to_ptr_name,
        types::{DnsName, Hostname, IpAddressValue, RecordTypeName},
    },
    errors::AppError,
    storage::HostStore,
};

use super::{
    MemoryState, MemoryStorage,
    attachments::{create_attachment_dhcp_identifier_in_state, find_or_create_attachment_in_state},
    bump_zone_serial_in_state, delete_records_by_name_and_type_in_state,
    delete_records_by_owner_in_state, paginate_by_cursor,
    records::create_record_in_state,
    sort_items,
};

fn create_host_in_state(state: &mut MemoryState, command: CreateHost) -> Result<Host, AppError> {
    let key = command.name().as_str().to_string();
    if state.hosts.contains_key(&key) {
        return Err(AppError::conflict(format!("host '{}' already exists", key)));
    }
    if let Some(zone) = command.zone()
        && !state.forward_zones.contains_key(zone.as_str())
    {
        return Err(AppError::not_found(format!(
            "forward zone '{}' was not found",
            zone.as_str()
        )));
    }
    let now = Utc::now();
    let host = Host::restore(
        Uuid::new_v4(),
        command.name().clone(),
        command.zone().cloned(),
        command.ttl(),
        command.comment().to_string(),
        now,
        now,
    )?;
    state.hosts.insert(key, host.clone());
    Ok(host)
}

pub(super) fn assign_ip_in_state(
    state: &mut MemoryState,
    command: AssignIpAddress,
) -> Result<IpAddressAssignment, AppError> {
    let host = state
        .hosts
        .get(command.host_name().as_str())
        .cloned()
        .ok_or_else(|| {
            AppError::not_found(format!(
                "host '{}' was not found",
                command.host_name().as_str()
            ))
        })?;

    let (network, address) = if let Some(address) = command.address().cloned() {
        let network = most_specific_network_for_address(state, &address)?;
        ensure_address_is_usable(state, &network, &address)?;
        (network, address)
    } else {
        let wanted_network = command.network().cloned().ok_or_else(|| {
            AppError::validation("automatic allocation requires a target network")
        })?;
        let network = state
            .networks
            .get(&wanted_network.as_str())
            .cloned()
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "network '{}' was not found",
                    wanted_network.as_str()
                ))
            })?;
        let address = allocate_address_in_network(state, &network)?;
        (network, address)
    };

    let key = address.as_str();
    if state.ip_addresses.contains_key(&key) {
        return Err(AppError::conflict(format!(
            "IP address '{}' is already allocated",
            key
        )));
    }

    let now = Utc::now();
    let attachment = find_or_create_attachment_in_state(
        state,
        host.name(),
        network.cidr(),
        command.mac_address().cloned(),
    )?;
    let assignment = IpAddressAssignment::restore(
        Uuid::new_v4(),
        host.id(),
        attachment.id(),
        address,
        network.id(),
        attachment.mac_address().cloned(),
        now,
        now,
    )?;
    state.ip_addresses.insert(key, assignment.clone());

    // Auto-create DHCP identifiers from MAC address
    if let Some(mac) = attachment.mac_address() {
        if assignment.family() == 4 && command.auto_v4_client_id() {
            let has_v4 = state
                .attachment_dhcp_identifiers
                .values()
                .any(|id| id.attachment_id() == attachment.id() && id.family().as_u8() == 4);
            if !has_v4 {
                let client_id_value = format!("01:{}", mac.as_str());
                create_attachment_dhcp_identifier_in_state(
                    state,
                    CreateAttachmentDhcpIdentifier::new(
                        attachment.id(),
                        DhcpIdentifierFamily::V4,
                        DhcpIdentifierKind::ClientId,
                        client_id_value,
                        1000,
                    )?,
                )?;
            }
        }
        if assignment.family() == 6 && command.auto_v6_duid_ll() {
            let has_v6 = state
                .attachment_dhcp_identifiers
                .values()
                .any(|id| id.attachment_id() == attachment.id() && id.family().as_u8() == 6);
            if !has_v6 {
                let duid_ll_value = format!("00:03:00:01:{}", mac.as_str());
                create_attachment_dhcp_identifier_in_state(
                    state,
                    CreateAttachmentDhcpIdentifier::new(
                        attachment.id(),
                        DhcpIdentifierFamily::V6,
                        DhcpIdentifierKind::DuidLl,
                        duid_ll_value,
                        1000,
                    )?,
                )?;
            }
        }
    }

    Ok(assignment)
}

fn most_specific_network_for_address(
    state: &MemoryState,
    address: &IpAddressValue,
) -> Result<Network, AppError> {
    state
        .networks
        .values()
        .filter(|network| network.contains(address))
        .max_by_key(|network| network.prefix_len())
        .cloned()
        .ok_or_else(|| {
            AppError::validation(format!(
                "IP address '{}' is not contained in any known network",
                address.as_str()
            ))
        })
}

fn ensure_address_is_usable(
    state: &MemoryState,
    network: &Network,
    address: &IpAddressValue,
) -> Result<(), AppError> {
    if !cidr_contains(network.cidr(), address) {
        return Err(AppError::validation(
            "IP address is outside the selected network",
        ));
    }
    let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
    let value = ip_to_u128(address.as_inner());
    if value < first || value > last {
        return Err(AppError::validation(
            "IP address falls inside reserved or unusable network space",
        ));
    }
    if state
        .excluded_ranges
        .get(&network.cidr().as_str())
        .into_iter()
        .flat_map(|ranges| ranges.iter())
        .any(|range| range.contains(address))
    {
        return Err(AppError::validation(
            "IP address falls inside an excluded range",
        ));
    }
    if state.ip_addresses.contains_key(&address.as_str()) {
        return Err(AppError::conflict(format!(
            "IP address '{}' is already allocated",
            address.as_str()
        )));
    }
    Ok(())
}

fn allocate_address_in_network(
    state: &MemoryState,
    network: &Network,
) -> Result<IpAddressValue, AppError> {
    let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
    match network.cidr().as_inner() {
        ipnet::IpNet::V4(_) => {
            for candidate in first..=last {
                let address =
                    IpAddressValue::new(std::net::Ipv4Addr::from(candidate as u32).to_string())?;
                if ensure_address_is_usable(state, network, &address).is_ok() {
                    return Ok(address);
                }
            }
            Err(AppError::conflict(
                "network has no remaining allocatable IPv4 addresses",
            ))
        }
        ipnet::IpNet::V6(_) => {
            for candidate in first..=last {
                let address = IpAddressValue::new(std::net::Ipv6Addr::from(candidate).to_string())?;
                if ensure_address_is_usable(state, network, &address).is_ok() {
                    return Ok(address);
                }
            }
            Err(AppError::conflict(
                "network has no remaining allocatable IPv6 addresses",
            ))
        }
    }
}

fn allocate_random_address_in_network(
    state: &MemoryState,
    network: &Network,
) -> Result<IpAddressValue, AppError> {
    use rand::Rng;

    let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;

    // Build the set of usable candidate values
    let candidates: Vec<u128> = (first..=last)
        .filter(|c| {
            let addr = match network.cidr().as_inner() {
                ipnet::IpNet::V4(_) => {
                    IpAddressValue::new(std::net::Ipv4Addr::from(*c as u32).to_string())
                }
                ipnet::IpNet::V6(_) => {
                    IpAddressValue::new(std::net::Ipv6Addr::from(*c).to_string())
                }
            };
            match addr {
                Ok(a) => ensure_address_is_usable(state, network, &a).is_ok(),
                Err(_) => false,
            }
        })
        .collect();

    if candidates.is_empty() {
        return Err(AppError::conflict(
            "network has no remaining allocatable addresses",
        ));
    }

    let mut rng = rand::thread_rng();
    let idx = rng.gen_range(0..candidates.len());
    let chosen = candidates[idx];

    match network.cidr().as_inner() {
        ipnet::IpNet::V4(_) => {
            IpAddressValue::new(std::net::Ipv4Addr::from(chosen as u32).to_string())
        }
        ipnet::IpNet::V6(_) => IpAddressValue::new(std::net::Ipv6Addr::from(chosen).to_string()),
    }
}

#[async_trait]
impl HostStore for MemoryStorage {
    async fn list_hosts(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
    ) -> Result<Page<Host>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<Host> = state
            .hosts
            .values()
            .filter(|host| filter.matches(host, &state.ip_addresses))
            .cloned()
            .collect();
        sort_items(&mut items, page, |host, field| match field {
            "comment" => host.comment().to_string(),
            "created_at" => host.created_at().to_rfc3339(),
            "updated_at" => host.updated_at().to_rfc3339(),
            _ => host.name().as_str().to_string(),
        });
        paginate_by_cursor(items, page)
    }

    async fn create_host(&self, command: CreateHost) -> Result<Host, AppError> {
        let mut state = self.state.write().await;
        let ip_specs = command.ip_assignments().to_vec();
        let host = create_host_in_state(&mut state, command)?;

        // Process IP assignments atomically within the same write lock.
        // If any assignment fails, roll back the host and any partial IP
        // assignments to simulate transactional semantics.
        let result = (|| -> Result<(), AppError> {
            for spec in &ip_specs {
                let assign_cmd = if *spec.allocation() == AllocationPolicy::Random {
                    if let Some(network_cidr) = spec.network() {
                        let network = state
                            .networks
                            .get(&network_cidr.as_str())
                            .cloned()
                            .ok_or_else(|| {
                                AppError::not_found(format!(
                                    "network '{}' was not found",
                                    network_cidr.as_str()
                                ))
                            })?;
                        let address = allocate_random_address_in_network(&state, &network)?;
                        let cmd = AssignIpAddress::new(
                            host.name().clone(),
                            Some(address),
                            None,
                            spec.mac_address().cloned(),
                        )?;
                        cmd.with_auto_dhcp(spec.auto_v4_client_id(), spec.auto_v6_duid_ll())
                    } else {
                        spec.clone().into_assign_command(host.name().clone())?
                    }
                } else {
                    spec.clone().into_assign_command(host.name().clone())?
                };
                let host_name = host.name().clone();
                let assignment = assign_ip_in_state(&mut state, assign_cmd)?;

                // Auto-create A/AAAA record
                let type_name = if assignment.family() == 4 {
                    "A"
                } else {
                    "AAAA"
                };
                let record_cmd = CreateRecordInstance::new(
                    RecordTypeName::new(type_name).unwrap(),
                    RecordOwnerKind::Host,
                    host_name.as_str(),
                    None,
                    json!({ "address": assignment.address().as_str() }),
                );
                if let Ok(cmd) = record_cmd {
                    create_record_in_state(&mut state, cmd)?;
                }

                // Auto-create PTR record if a matching reverse zone exists
                let ptr_name = ip_to_ptr_name(assignment.address());
                let has_matching_rz = state.reverse_zones.values().any(|rz| {
                    rz.network().is_some_and(|net| {
                        net.as_inner().contains(&assignment.address().as_inner())
                    })
                });
                if has_matching_rz {
                    let ptr_cmd = CreateRecordInstance::new(
                        RecordTypeName::new("PTR").unwrap(),
                        RecordOwnerKind::ReverseZone,
                        &ptr_name,
                        None,
                        json!({ "ptrdname": host_name.as_str() }),
                    );
                    if let Ok(cmd) = ptr_cmd {
                        let _ = create_record_in_state(&mut state, cmd);
                    }
                }
            }
            Ok(())
        })();

        if let Err(err) = result {
            // Roll back: remove any IP assignments and the host itself
            let host_id = host.id();
            state.ip_addresses.retain(|_, a| a.host_id() != host_id);
            delete_records_by_owner_in_state(&mut state, host_id);
            state.hosts.remove(host.name().as_str());
            return Err(err);
        }

        Ok(host)
    }

    async fn get_host_by_name(&self, name: &Hostname) -> Result<Host, AppError> {
        let state = self.state.read().await;
        state
            .hosts
            .get(name.as_str())
            .cloned()
            .ok_or_else(|| AppError::not_found(format!("host '{}' was not found", name.as_str())))
    }

    async fn get_host_auth_context(&self, name: &Hostname) -> Result<HostAuthContext, AppError> {
        let state = self.state.read().await;
        let host = state.hosts.get(name.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("host '{}' was not found", name.as_str()))
        })?;

        let mut addresses = Vec::new();
        let mut seen_networks = std::collections::BTreeSet::new();
        let mut networks = Vec::new();
        for assignment in state.ip_addresses.values() {
            if assignment.host_id() != host.id() {
                continue;
            }
            addresses.push(*assignment.address());
            if seen_networks.insert(assignment.network_id()) {
                let cidr = state
                    .networks
                    .values()
                    .find(|network| network.id() == assignment.network_id())
                    .ok_or_else(|| {
                        AppError::internal(format!(
                            "host '{}' references unknown network id '{}'",
                            name.as_str(),
                            assignment.network_id()
                        ))
                    })?;
                networks.push(cidr.cidr().clone());
            }
        }
        addresses.sort_by_key(|address| address.as_str());
        networks.sort_by_key(|network| network.as_str().to_string());

        Ok(HostAuthContext::new(host, addresses, networks))
    }

    async fn update_host(&self, name: &Hostname, command: UpdateHost) -> Result<Host, AppError> {
        let mut state = self.state.write().await;
        let host = state.hosts.get(name.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("host '{}' was not found", name.as_str()))
        })?;
        let now = Utc::now();
        let new_name = command.name.unwrap_or_else(|| host.name().clone());
        let ttl = match command.ttl {
            Some(new_ttl) => new_ttl,
            None => host.ttl(),
        };
        let comment = command
            .comment
            .unwrap_or_else(|| host.comment().to_string());
        let zone = match command.zone {
            Some(new_zone) => new_zone,
            None => host.zone().cloned(),
        };
        if let Some(ref z) = zone
            && !state.forward_zones.contains_key(z.as_str())
        {
            return Err(AppError::not_found(format!(
                "forward zone '{}' was not found",
                z.as_str()
            )));
        }
        let updated = Host::restore(
            host.id(),
            new_name.clone(),
            zone,
            ttl,
            comment,
            host.created_at(),
            now,
        )?;
        if new_name.as_str() != name.as_str() {
            state.hosts.remove(name.as_str());
            // Cascade rename to records and rrsets
            if let Ok(new_dns_name) = DnsName::new(new_name.as_str()) {
                let now = Utc::now();
                for record in &mut state.records {
                    if record.owner_id() == Some(host.id()) {
                        *record = RecordInstance::restore(
                            record.id(),
                            record.rrset_id(),
                            record.type_id(),
                            record.type_name().clone(),
                            record.owner_kind().cloned(),
                            record.owner_id(),
                            new_dns_name.clone(),
                            record.zone_id(),
                            record.ttl(),
                            record.data().clone(),
                            record.raw_rdata().cloned(),
                            record.rendered().map(str::to_string),
                            record.created_at(),
                            now,
                        );
                    }
                }
                let rrset_ids: Vec<Uuid> = state
                    .rrsets
                    .values()
                    .filter(|rs| rs.anchor_id() == Some(host.id()))
                    .map(|rs| rs.id())
                    .collect();
                for rrset_id in rrset_ids {
                    if let Some(rrset) = state.rrsets.remove(&rrset_id) {
                        let updated_rrset = RecordRrset::restore(
                            rrset.id(),
                            rrset.type_id(),
                            rrset.type_name().clone(),
                            rrset.dns_class().clone(),
                            new_dns_name.clone(),
                            rrset.anchor_kind().cloned(),
                            rrset.anchor_id(),
                            Some(new_name.as_str().to_string()),
                            rrset.zone_id(),
                            rrset.ttl(),
                            rrset.created_at(),
                            rrset.updated_at(),
                        );
                        state.rrsets.insert(rrset_id, updated_rrset);
                    }
                }
            }
            // Bump zone serial after rename (DNS-visible change)
            if let Some(zone_name) = updated.zone() {
                let zone_id = state.forward_zones.get(zone_name.as_str()).map(|z| z.id());
                if let Some(zone_id) = zone_id {
                    bump_zone_serial_in_state(&mut state, zone_id);
                }
            }
        }
        state
            .hosts
            .insert(new_name.as_str().to_string(), updated.clone());
        Ok(updated)
    }

    async fn delete_host(&self, name: &Hostname) -> Result<(), AppError> {
        let mut state = self.state.write().await;
        let host = state.hosts.get(name.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("host '{}' was not found", name.as_str()))
        })?;
        // Cascade: delete all records owned by this host
        delete_records_by_owner_in_state(&mut state, host.id());
        // Cascade: bump zone serial
        if let Some(zone_name) = host.zone()
            && let Some(zone) = state.forward_zones.get(zone_name.as_str())
        {
            let zone_id = zone.id();
            bump_zone_serial_in_state(&mut state, zone_id);
        }
        state.hosts.remove(name.as_str());
        state
            .ip_addresses
            .retain(|_, assignment| assignment.host_id() != host.id());
        Ok(())
    }

    async fn list_ip_addresses(
        &self,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        let state = self.state.read().await;
        let mut items: Vec<IpAddressAssignment> = state.ip_addresses.values().cloned().collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn list_ip_addresses_for_host(
        &self,
        host: &Hostname,
        page: &PageRequest,
    ) -> Result<Page<IpAddressAssignment>, AppError> {
        let state = self.state.read().await;
        let host = state.hosts.get(host.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("host '{}' was not found", host.as_str()))
        })?;
        let mut items: Vec<IpAddressAssignment> = state
            .ip_addresses
            .values()
            .filter(|assignment| assignment.host_id() == host.id())
            .cloned()
            .collect();
        items.sort_by_key(|item| item.id());
        paginate_by_cursor(items, page)
    }

    async fn assign_ip_address(
        &self,
        command: AssignIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        let mut state = self.state.write().await;
        let host_name = command.host_name().clone();
        let assignment = assign_ip_in_state(&mut state, command)?;

        // Auto-create A/AAAA record
        let type_name = if assignment.family() == 4 {
            "A"
        } else {
            "AAAA"
        };
        let record_cmd = CreateRecordInstance::new(
            RecordTypeName::new(type_name).unwrap(),
            RecordOwnerKind::Host,
            host_name.as_str(),
            None,
            json!({ "address": assignment.address().as_str() }),
        );
        if let Ok(cmd) = record_cmd {
            create_record_in_state(&mut state, cmd)?;
        }

        // Auto-create PTR record if a matching reverse zone exists
        let ptr_name = ip_to_ptr_name(assignment.address());
        let has_matching_rz = state.reverse_zones.values().any(|rz| {
            rz.network()
                .is_some_and(|net| net.as_inner().contains(&assignment.address().as_inner()))
        });
        if has_matching_rz {
            let ptr_cmd = CreateRecordInstance::new(
                RecordTypeName::new("PTR").unwrap(),
                RecordOwnerKind::ReverseZone,
                &ptr_name,
                None,
                json!({ "ptrdname": host_name.as_str() }),
            );
            if let Ok(cmd) = ptr_cmd {
                let _ = create_record_in_state(&mut state, cmd);
            }
        }

        Ok(assignment)
    }

    async fn update_ip_address(
        &self,
        address: &IpAddressValue,
        command: UpdateIpAddress,
    ) -> Result<IpAddressAssignment, AppError> {
        let mut state = self.state.write().await;
        let key = address.as_str();
        let existing = state.ip_addresses.get(&key).cloned().ok_or_else(|| {
            AppError::not_found(format!("IP address assignment '{}' was not found", key))
        })?;
        let now = Utc::now();
        let mac = match command.mac_address {
            Some(new_mac) => new_mac,
            None => existing.mac_address().cloned(),
        };
        let updated = IpAddressAssignment::restore(
            existing.id(),
            existing.host_id(),
            existing.attachment_id(),
            *existing.address(),
            existing.network_id(),
            mac,
            existing.created_at(),
            now,
        )?;
        state.ip_addresses.insert(key.clone(), updated.clone());
        Ok(updated)
    }

    async fn unassign_ip_address(
        &self,
        address: &IpAddressValue,
    ) -> Result<IpAddressAssignment, AppError> {
        let mut state = self.state.write().await;
        let key = address.as_str();
        let assignment = state.ip_addresses.remove(&key).ok_or_else(|| {
            AppError::not_found(format!("IP address assignment '{}' was not found", key))
        })?;

        // Find the host name for record cleanup
        let host_name = state
            .hosts
            .values()
            .find(|h| h.id() == assignment.host_id())
            .map(|h| h.name().as_str().to_string());

        if let Some(host_name) = host_name {
            // Delete matching A/AAAA record (only the one with matching address data)
            let type_name = if assignment.family() == 4 {
                "A"
            } else {
                "AAAA"
            };
            let addr_str = assignment.address().as_str();
            let mut kept = Vec::new();
            let mut removed = Vec::new();
            for record in state.records.drain(..) {
                if record.owner_name().eq_ignore_ascii_case(&host_name)
                    && record.type_name().as_str() == type_name
                    && record
                        .data()
                        .get("address")
                        .and_then(|v| v.as_str())
                        .is_some_and(|a| a == addr_str)
                {
                    removed.push(record);
                } else {
                    kept.push(record);
                }
            }
            state.records = kept;
            let rrset_ids: HashSet<Uuid> = removed.iter().map(|r| r.rrset_id()).collect();
            for rrset_id in rrset_ids {
                if !state.records.iter().any(|r| r.rrset_id() == rrset_id) {
                    state.rrsets.remove(&rrset_id);
                }
            }
        }

        // Delete PTR record for this address
        let ptr_name = ip_to_ptr_name(assignment.address());
        delete_records_by_name_and_type_in_state(&mut state, &ptr_name, "PTR");

        Ok(assignment)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{Config, StorageBackendSetting},
        domain::{
            host::{AssignIpAddress, CreateHost},
            network::CreateNetwork,
            types::{CidrValue, Hostname, IpAddressValue},
        },
        storage::build_storage,
    };

    #[tokio::test]
    async fn host_auth_context_includes_attached_networks() {
        let storage = build_storage(&Config {
            workers: Some(1),
            run_migrations: false,
            storage_backend: StorageBackendSetting::Memory,
            treetop_timeout_ms: 1000,
            allow_dev_authz_bypass: false,
            ..Config::default()
        })
        .expect("memory storage should build");

        let network = CidrValue::new("10.250.1.0/24").expect("valid cidr");
        storage
            .networks()
            .create_network(
                CreateNetwork::new(network.clone(), "legacy authz network", 1)
                    .expect("valid network"),
            )
            .await
            .expect("network should be created");

        let host = Hostname::new("legacy-authz.example.test").expect("valid hostname");
        storage
            .hosts()
            .create_host(CreateHost::new(host.clone(), None, None, "legacy authz host").unwrap())
            .await
            .expect("host should be created");

        storage
            .hosts()
            .assign_ip_address(
                AssignIpAddress::new(
                    host.clone(),
                    Some(IpAddressValue::new("10.250.1.10").expect("valid ip")),
                    None,
                    None,
                )
                .expect("valid assignment"),
            )
            .await
            .expect("assignment should succeed");

        let context = storage
            .hosts()
            .get_host_auth_context(&host)
            .await
            .expect("auth context should load");

        assert_eq!(context.addresses().len(), 1);
        assert_eq!(context.networks(), &[network]);
    }
}
