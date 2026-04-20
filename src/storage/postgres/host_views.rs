use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::{
    PgConnection, QueryableByName,
    sql_types::{Array, BigInt, Integer, Jsonb, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            HostAttachment,
        },
        filters::HostFilter,
        host::{Host, IpAddressAssignment},
        host_view::{
            HostAttachmentView, HostDnsRecordView, HostInventoryView, HostPolicyView, HostView,
            HostViewExpansions,
        },
        pagination::{Page, PageRequest},
        types::{
            CidrValue, CommunityName, DhcpPriority, Hostname, IpAddressValue, MacAddressValue,
            NetworkPolicyName, Ttl, ZoneName,
        },
    },
    errors::AppError,
    storage::{HostStore, HostViewStore, postgres::PostgresStorage},
};

use super::helpers::{rows_to_page, run_dynamic_query};

#[derive(QueryableByName)]
struct HostViewRow {
    #[diesel(sql_type = SqlUuid)]
    id: Uuid,
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Nullable<Text>)]
    zone_name: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    ttl: Option<i32>,
    #[diesel(sql_type = Text)]
    comment: String,
    #[diesel(sql_type = Timestamptz)]
    created_at: DateTime<Utc>,
    #[diesel(sql_type = Timestamptz)]
    updated_at: DateTime<Utc>,
    #[diesel(sql_type = BigInt)]
    total_count: i64,
    #[diesel(sql_type = Jsonb)]
    attachments: Value,
    #[diesel(sql_type = Array<Text>)]
    contacts: Vec<String>,
    #[diesel(sql_type = Array<Text>)]
    groups: Vec<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    bacnet_id: Option<i32>,
    #[diesel(sql_type = Jsonb)]
    dns_records: Value,
    #[diesel(sql_type = Array<Text>)]
    roles: Vec<String>,
    #[diesel(sql_type = Array<Text>)]
    atoms: Vec<String>,
}

#[derive(Deserialize)]
struct HostAttachmentJson {
    id: Uuid,
    network_id: Uuid,
    network: String,
    mac_address: Option<String>,
    comment: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    ip_addresses: Vec<IpAddressJson>,
    dhcp_identifiers: Vec<DhcpIdentifierJson>,
    prefix_reservations: Vec<PrefixReservationJson>,
    community_assignments: Vec<CommunityAssignmentJson>,
}

#[derive(Deserialize)]
struct IpAddressJson {
    id: Uuid,
    address: String,
    mac_address: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct DhcpIdentifierJson {
    id: Uuid,
    family: String,
    kind: String,
    value: String,
    priority: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct PrefixReservationJson {
    id: Uuid,
    prefix: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct CommunityAssignmentJson {
    id: Uuid,
    community_id: Uuid,
    community_name: String,
    policy_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct DnsRecordJson {
    id: Uuid,
    type_name: String,
    ttl: Option<i32>,
    rendered: Option<String>,
}

impl HostViewRow {
    fn into_domain(self, expansions: HostViewExpansions) -> Result<HostView, AppError> {
        let host = Host::restore(
            self.id,
            Hostname::new(self.name)?,
            self.zone_name.map(ZoneName::new).transpose()?,
            self.ttl
                .map(|ttl| {
                    Ttl::new(
                        u32::try_from(ttl)
                            .map_err(|_| AppError::internal("invalid TTL value in host view"))?,
                    )
                })
                .transpose()?,
            self.comment,
            self.created_at,
            self.updated_at,
        )?;
        let mut view = HostView::new(host.clone());

        if expansions.attachments {
            let attachments: Vec<HostAttachmentJson> =
                serde_json::from_value(self.attachments).map_err(AppError::internal)?;
            view.attachments = attachments
                .into_iter()
                .map(|attachment| attachment.into_domain(&host))
                .collect::<Result<Vec<_>, _>>()?;
        }

        if expansions.inventory {
            view.inventory = HostInventoryView {
                contacts: self.contacts,
                groups: self.groups,
                bacnet_id: self.bacnet_id.map(|value| value as u32),
            };
        }

        if expansions.dns_records {
            let records: Vec<DnsRecordJson> =
                serde_json::from_value(self.dns_records).map_err(AppError::internal)?;
            view.dns_records = records
                .into_iter()
                .map(|record| {
                    Ok(HostDnsRecordView {
                        id: record.id,
                        type_name: record.type_name,
                        ttl: record
                            .ttl
                            .map(|value| {
                                u32::try_from(value).map_err(|_| {
                                    AppError::internal("invalid record TTL value in host view")
                                })
                            })
                            .transpose()?,
                        rendered: record.rendered,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
        }

        if expansions.host_policy {
            view.host_policy = HostPolicyView {
                roles: self.roles,
                atoms: self.atoms,
            };
        }

        Ok(view)
    }
}

impl HostAttachmentJson {
    fn into_domain(self, host: &Host) -> Result<HostAttachmentView, AppError> {
        let network_cidr = CidrValue::new(self.network)?;
        let attachment = HostAttachment::restore(
            self.id,
            host.id(),
            host.name().clone(),
            self.network_id,
            network_cidr.clone(),
            self.mac_address.map(MacAddressValue::new).transpose()?,
            self.comment,
            self.created_at,
            self.updated_at,
        );

        let ip_addresses = self
            .ip_addresses
            .into_iter()
            .map(|ip| {
                IpAddressAssignment::restore(
                    ip.id,
                    host.id(),
                    self.id,
                    IpAddressValue::new(ip.address)?,
                    self.network_id,
                    ip.mac_address.map(MacAddressValue::new).transpose()?,
                    ip.created_at,
                    ip.updated_at,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let dhcp_identifiers = self
            .dhcp_identifiers
            .into_iter()
            .map(|identifier| {
                AttachmentDhcpIdentifier::restore(
                    identifier.id,
                    self.id,
                    serde_json::from_value(Value::String(identifier.family))
                        .map_err(AppError::internal)?,
                    serde_json::from_value(Value::String(identifier.kind))
                        .map_err(AppError::internal)?,
                    identifier.value,
                    DhcpPriority::new(identifier.priority),
                    identifier.created_at,
                    identifier.updated_at,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let prefix_reservations = self
            .prefix_reservations
            .into_iter()
            .map(|reservation| {
                AttachmentPrefixReservation::restore(
                    reservation.id,
                    self.id,
                    CidrValue::new(reservation.prefix)?,
                    reservation.created_at,
                    reservation.updated_at,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let community_assignments = self
            .community_assignments
            .into_iter()
            .map(|assignment| {
                Ok(AttachmentCommunityAssignment::restore(
                    assignment.id,
                    self.id,
                    host.id(),
                    host.name().clone(),
                    self.network_id,
                    network_cidr.clone(),
                    assignment.community_id,
                    CommunityName::new(assignment.community_name)?,
                    NetworkPolicyName::new(assignment.policy_name)?,
                    assignment.created_at,
                    assignment.updated_at,
                ))
            })
            .collect::<Result<Vec<_>, AppError>>()?;

        Ok(HostAttachmentView {
            attachment,
            ip_addresses,
            dhcp_identifiers,
            prefix_reservations,
            community_assignments,
        })
    }
}

impl PostgresStorage {
    fn host_view_order_expr(page: &PageRequest) -> Result<&'static str, AppError> {
        match page.sort_by() {
            Some("comment") => Ok("fh.comment"),
            Some("created_at") => Ok("fh.created_at"),
            Some("updated_at") => Ok("fh.updated_at"),
            Some("name") | None => Ok("fh.name"),
            Some(other) => Err(AppError::validation(format!(
                "unsupported sort_by field for hosts: {other}"
            ))),
        }
    }

    fn query_host_views_detail(
        connection: &mut PgConnection,
        page: &PageRequest,
        filter: &HostFilter,
        exact_name: Option<&Hostname>,
        expansions: HostViewExpansions,
    ) -> Result<Page<HostView>, AppError> {
        let mut clauses_and_values = filter.sql_conditions();
        if let Some(name) = exact_name {
            clauses_and_values
                .0
                .push(format!("h.name = ${}", clauses_and_values.1.len() + 1));
            clauses_and_values.1.push(name.as_str().to_string());
        }
        let (clauses, values) = clauses_and_values;
        let where_str = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };

        let order_expr = Self::host_view_order_expr(page)?;
        let order_dir = match page.sort_direction() {
            crate::domain::pagination::SortDirection::Asc => "ASC",
            crate::domain::pagination::SortDirection::Desc => "DESC",
        };
        let limit_clause = if page.after().is_none() && page.limit() != u64::MAX {
            format!(" LIMIT {}", page.limit() + 1)
        } else {
            String::new()
        };

        let attachments_sql = if expansions.attachments {
            r#"
            COALESCE(att.attachments, '[]'::jsonb) AS attachments
            "#
        } else {
            "'[]'::jsonb AS attachments"
        };
        let contacts_sql = if expansions.inventory {
            "COALESCE(ct.contacts, ARRAY[]::text[]) AS contacts"
        } else {
            "ARRAY[]::text[] AS contacts"
        };
        let groups_sql = if expansions.inventory {
            "COALESCE(gr.groups, ARRAY[]::text[]) AS groups"
        } else {
            "ARRAY[]::text[] AS groups"
        };
        let bacnet_sql = if expansions.inventory {
            "bac.bacnet_id AS bacnet_id"
        } else {
            "NULL::int AS bacnet_id"
        };
        let records_sql = if expansions.dns_records {
            "COALESCE(rec.dns_records, '[]'::jsonb) AS dns_records"
        } else {
            "'[]'::jsonb AS dns_records"
        };
        let roles_sql = if expansions.host_policy {
            "COALESCE(pol.roles, ARRAY[]::text[]) AS roles"
        } else {
            "ARRAY[]::text[] AS roles"
        };
        let atoms_sql = if expansions.host_policy {
            "COALESCE(pol.atoms, ARRAY[]::text[]) AS atoms"
        } else {
            "ARRAY[]::text[] AS atoms"
        };

        let sql = format!(
            r#"
            WITH filtered_hosts AS (
                SELECT h.id,
                       h.name::text AS name,
                       fz.name::text AS zone_name,
                       h.ttl,
                       h.comment,
                       h.created_at,
                       h.updated_at
                FROM hosts h
                LEFT JOIN forward_zones fz ON fz.id = h.zone_id
                {where_str}
            ),
            paged_hosts AS (
                SELECT fh.*,
                       COUNT(*) OVER() AS total_count,
                       ROW_NUMBER() OVER (ORDER BY {order_expr} {order_dir}, fh.id) AS ord
                FROM filtered_hosts fh
                ORDER BY {order_expr} {order_dir}, fh.id
                {limit_clause}
            )
            SELECT ph.id,
                   ph.name,
                   ph.zone_name,
                   ph.ttl,
                   ph.comment,
                   ph.created_at,
                   ph.updated_at,
                   ph.total_count,
                   {attachments_sql},
                   {contacts_sql},
                   {groups_sql},
                   {bacnet_sql},
                   {records_sql},
                   {roles_sql},
                   {atoms_sql}
            FROM paged_hosts ph
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                           jsonb_build_object(
                               'id', a.id,
                               'network_id', a.network_id,
                               'network', n.network::text,
                               'mac_address', a.mac_address,
                               'comment', a.comment,
                               'created_at', a.created_at,
                               'updated_at', a.updated_at,
                               'ip_addresses', COALESCE(ip.ip_addresses, '[]'::jsonb),
                               'dhcp_identifiers', COALESCE(dhcp.identifiers, '[]'::jsonb),
                               'prefix_reservations', COALESCE(pref.reservations, '[]'::jsonb),
                               'community_assignments', COALESCE(comm.assignments, '[]'::jsonb)
                           )
                           ORDER BY n.network::text, a.mac_address NULLS LAST, a.id
                       ) AS attachments
                FROM host_attachments a
                JOIN networks n ON n.id = a.network_id
                LEFT JOIN LATERAL (
                    SELECT jsonb_agg(
                               jsonb_build_object(
                                   'id', ip.id,
                                   'address', host(ip.address),
                                   'mac_address', ip.mac_address,
                                   'created_at', ip.created_at,
                                   'updated_at', ip.updated_at
                               )
                               ORDER BY host(ip.address), ip.id
                           ) AS ip_addresses
                    FROM ip_addresses ip
                    WHERE ip.attachment_id = a.id
                ) ip ON true
                LEFT JOIN LATERAL (
                    SELECT jsonb_agg(
                               jsonb_build_object(
                                   'id', adi.id,
                                   'family', CASE adi.family::int WHEN 4 THEN 'v4' ELSE 'v6' END,
                                   'kind', adi.kind,
                                   'value', adi.value,
                                   'priority', adi.priority,
                                   'created_at', adi.created_at,
                                   'updated_at', adi.updated_at
                               )
                               ORDER BY adi.family, adi.priority, adi.kind, adi.value, adi.id
                           ) AS identifiers
                    FROM attachment_dhcp_identifiers adi
                    WHERE adi.attachment_id = a.id
                ) dhcp ON true
                LEFT JOIN LATERAL (
                    SELECT jsonb_agg(
                               jsonb_build_object(
                                   'id', apr.id,
                                   'prefix', apr.prefix::text,
                                   'created_at', apr.created_at,
                                   'updated_at', apr.updated_at
                               )
                               ORDER BY apr.prefix::text, apr.id
                           ) AS reservations
                    FROM attachment_prefix_reservations apr
                    WHERE apr.attachment_id = a.id
                ) pref ON true
                LEFT JOIN LATERAL (
                    SELECT jsonb_agg(
                               jsonb_build_object(
                                   'id', aca.id,
                                   'community_id', c.id,
                                   'community_name', c.name::text,
                                   'policy_name', np.name::text,
                                   'created_at', aca.created_at,
                                   'updated_at', aca.updated_at
                               )
                               ORDER BY np.name::text, c.name::text, aca.id
                           ) AS assignments
                    FROM attachment_community_assignments aca
                    JOIN communities c ON c.id = aca.community_id
                    JOIN network_policies np ON np.id = c.policy_id
                    WHERE aca.attachment_id = a.id
                ) comm ON true
                WHERE a.host_id = ph.id
            ) att ON true
            LEFT JOIN LATERAL (
                SELECT array_agg(DISTINCT hc.email::text ORDER BY hc.email::text) AS contacts
                FROM host_contacts hc
                JOIN host_contacts_hosts hch ON hch.contact_id = hc.id
                WHERE hch.host_id = ph.id
            ) ct ON true
            LEFT JOIN LATERAL (
                SELECT array_agg(DISTINCT hg.name::text ORDER BY hg.name::text) AS groups
                FROM host_groups hg
                JOIN host_group_hosts hgh ON hgh.host_group_id = hg.id
                WHERE hgh.host_id = ph.id
            ) gr ON true
            LEFT JOIN LATERAL (
                SELECT b.id AS bacnet_id
                FROM bacnet_ids b
                WHERE b.host_id = ph.id
                LIMIT 1
            ) bac ON true
            LEFT JOIN LATERAL (
                SELECT jsonb_agg(
                           jsonb_build_object(
                               'id', r.id,
                               'type_name', rt.name::text,
                               'ttl', rs.ttl,
                               'rendered', r.rendered
                           )
                           ORDER BY r.created_at DESC, r.id
                       ) AS dns_records
                FROM records r
                JOIN rrsets rs ON rs.id = r.rrset_id
                JOIN record_types rt ON rt.id = rs.type_id
                WHERE rs.anchor_kind = 'host'
                  AND rs.owner_name = ph.name
            ) rec ON true
            LEFT JOIN LATERAL (
                SELECT array_agg(DISTINCT r.name::text ORDER BY r.name::text) AS roles,
                       array_agg(DISTINCT a.name::text ORDER BY a.name::text)
                           FILTER (WHERE a.name IS NOT NULL) AS atoms
                FROM host_policy_role_hosts rh
                JOIN host_policy_roles r ON r.id = rh.role_id
                LEFT JOIN host_policy_role_atoms ra ON ra.role_id = r.id
                LEFT JOIN host_policy_atoms a ON a.id = ra.atom_id
                WHERE rh.host_id = ph.id
            ) pol ON true
            ORDER BY ph.ord
            "#
        );

        let rows = run_dynamic_query::<HostViewRow>(connection, &sql, &values)
            .map_err(AppError::internal)?;
        let total = rows
            .first()
            .map(|row| row.total_count.max(0) as u64)
            .unwrap_or(0);
        let items = rows
            .into_iter()
            .map(|row| row.into_domain(expansions))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows_to_page(items, page, total))
    }
}

#[async_trait]
impl HostViewStore for PostgresStorage {
    async fn list_host_views(
        &self,
        page: &PageRequest,
        filter: &HostFilter,
        expansions: HostViewExpansions,
    ) -> Result<Page<HostView>, AppError> {
        if expansions == HostViewExpansions::summary() {
            let page_hosts = HostStore::list_hosts(self, page, filter).await?;
            return Ok(Page {
                total: page_hosts.total,
                next_cursor: page_hosts.next_cursor,
                items: page_hosts.items.into_iter().map(HostView::new).collect(),
            });
        }

        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| {
                Self::query_host_views_detail(connection, &page, &filter, None, expansions)
            })
            .await
    }

    async fn get_host_view(
        &self,
        name: &Hostname,
        expansions: HostViewExpansions,
    ) -> Result<HostView, AppError> {
        if expansions == HostViewExpansions::summary() {
            let host = HostStore::get_host_by_name(self, name).await?;
            return Ok(HostView::new(host));
        }

        let name = name.clone();
        self.database
            .run(move |connection| {
                let views = Self::query_host_views_detail(
                    connection,
                    &PageRequest::all(),
                    &HostFilter::default(),
                    Some(&name),
                    expansions,
                )?;
                views.items.into_iter().next().ok_or_else(|| {
                    AppError::not_found(format!("host '{}' was not found", name.as_str()))
                })
            })
            .await
    }
}
