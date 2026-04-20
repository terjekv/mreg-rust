use std::collections::BTreeSet;

use async_trait::async_trait;

use crate::{
    domain::{
        filters::HostFilter,
        host::Host,
        host_view::{
            HostAttachmentView, HostDnsRecordView, HostInventoryView, HostPolicyView, HostView,
            HostViewExpansions,
        },
        pagination::{Page, PageRequest},
        resource_records::RecordOwnerKind,
        types::Hostname,
    },
    errors::AppError,
    storage::HostViewStore,
};

use super::{MemoryState, MemoryStorage, paginate_by_cursor, sort_items};

fn build_host_view(state: &MemoryState, host: &Host, expansions: HostViewExpansions) -> HostView {
    let mut view = HostView::new(host.clone());

    if expansions.attachments {
        view.attachments = state
            .host_attachments
            .values()
            .filter(|attachment| attachment.host_id() == host.id())
            .cloned()
            .map(|attachment| {
                let mut ip_addresses = state
                    .ip_addresses
                    .values()
                    .filter(|assignment| assignment.attachment_id() == attachment.id())
                    .cloned()
                    .collect::<Vec<_>>();
                ip_addresses.sort_by_key(|assignment| assignment.address().as_str().to_string());

                let mut dhcp_identifiers = state
                    .attachment_dhcp_identifiers
                    .values()
                    .filter(|identifier| identifier.attachment_id() == attachment.id())
                    .cloned()
                    .collect::<Vec<_>>();
                dhcp_identifiers.sort_by_key(|identifier| {
                    (
                        identifier.family().as_u8(),
                        identifier.priority().as_i32(),
                        format!("{:?}", identifier.kind()),
                        identifier.value().to_string(),
                    )
                });

                let mut prefix_reservations = state
                    .attachment_prefix_reservations
                    .values()
                    .filter(|reservation| reservation.attachment_id() == attachment.id())
                    .cloned()
                    .collect::<Vec<_>>();
                prefix_reservations
                    .sort_by_key(|reservation| reservation.prefix().as_str().to_string());

                let mut community_assignments = state
                    .attachment_community_assignments
                    .values()
                    .filter(|assignment| assignment.attachment_id() == attachment.id())
                    .cloned()
                    .collect::<Vec<_>>();
                community_assignments.sort_by_key(|assignment| {
                    (
                        assignment.policy_name().as_str().to_string(),
                        assignment.community_name().as_str().to_string(),
                    )
                });

                HostAttachmentView {
                    attachment,
                    ip_addresses,
                    dhcp_identifiers,
                    prefix_reservations,
                    community_assignments,
                }
            })
            .collect();
    }

    if expansions.inventory {
        let mut contacts = state
            .host_contacts
            .values()
            .filter(|contact| contact.hosts().iter().any(|value| value == host.name()))
            .map(|contact| contact.email().as_str().to_string())
            .collect::<Vec<_>>();
        contacts.sort();

        let mut groups = state
            .host_groups
            .values()
            .filter(|group| group.hosts().iter().any(|value| value == host.name()))
            .map(|group| group.name().as_str().to_string())
            .collect::<Vec<_>>();
        groups.sort();

        view.inventory = HostInventoryView {
            contacts,
            groups,
            bacnet_id: state
                .bacnet_ids
                .values()
                .find(|assignment| assignment.host_name() == host.name())
                .map(|assignment| assignment.bacnet_id().as_u32()),
        };
    }

    if expansions.dns_records {
        view.dns_records = state
            .records
            .iter()
            .filter(|record| {
                record.owner_kind() == Some(&RecordOwnerKind::Host)
                    && record.owner_name() == host.name().as_str()
            })
            .map(|record| HostDnsRecordView {
                id: record.id(),
                type_name: record.type_name().as_str().to_string(),
                ttl: record.ttl().map(|ttl| ttl.as_u32()),
                rendered: record.rendered().map(str::to_string),
            })
            .collect();
    }

    if expansions.host_policy {
        let mut roles = Vec::new();
        let mut atoms = BTreeSet::new();
        for role in state.host_policy_roles.values().filter(|role| {
            role.hosts()
                .iter()
                .any(|value| value == host.name().as_str())
        }) {
            roles.push(role.name().as_str().to_string());
            for atom in role.atoms() {
                atoms.insert(atom.to_string());
            }
        }
        roles.sort();
        view.host_policy = HostPolicyView {
            roles,
            atoms: atoms.into_iter().collect(),
        };
    }

    view
}

#[async_trait]
impl HostViewStore for MemoryStorage {
    async fn list_host_views(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
        expansions: HostViewExpansions,
    ) -> Result<Page<HostView>, AppError> {
        let state = self.state.read().await;
        let mut hosts: Vec<Host> = state
            .hosts
            .values()
            .filter(|host| filter.matches(host, &state.ip_addresses))
            .cloned()
            .collect();
        sort_items(
            &mut hosts,
            page,
            &["name", "comment", "created_at", "updated_at"],
            |host, field| match field {
                "comment" => host.comment().to_string(),
                "created_at" => host.created_at().to_rfc3339(),
                "updated_at" => host.updated_at().to_rfc3339(),
                _ => host.name().as_str().to_string(),
            },
        )?;
        let page_hosts = paginate_by_cursor(hosts, page)?;
        let items = page_hosts
            .items
            .iter()
            .map(|host| build_host_view(&state, host, expansions))
            .collect();
        Ok(Page {
            items,
            total: page_hosts.total,
            next_cursor: page_hosts.next_cursor,
        })
    }

    async fn get_host_view(
        &self,
        name: &Hostname,
        expansions: HostViewExpansions,
    ) -> Result<HostView, AppError> {
        let state = self.state.read().await;
        let host = state.hosts.get(name.as_str()).cloned().ok_or_else(|| {
            AppError::not_found(format!("host '{}' was not found", name.as_str()))
        })?;
        Ok(build_host_view(&state, &host, expansions))
    }
}
