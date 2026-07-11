use async_trait::async_trait;
use diesel::{
    ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl, sql_query,
    sql_types::{Array, Integer, Nullable, Text, Uuid as SqlUuid},
};
use uuid::Uuid;

use crate::{
    db::{
        models::{
            AttachmentCommunityAssignmentRow, AttachmentDhcpIdentifierRow,
            AttachmentPrefixReservationRow, HostAttachmentRow,
        },
        schema::{
            attachment_community_assignments, attachment_dhcp_identifiers,
            attachment_prefix_reservations, host_attachments, hosts, ip_addresses,
        },
    },
    domain::{
        attachment::{
            AttachmentCommunityAssignment, AttachmentDhcpIdentifier, AttachmentPrefixReservation,
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, DhcpIdentifierKind,
            HostAttachment, UpdateHostAttachment, validate_prefix_reservation_for_attachment,
        },
        filters::AttachmentCommunityAssignmentFilter,
        pagination::{Page, PageRequest},
        types::{CidrValue, Hostname, MacAddressValue},
    },
    errors::AppError,
    storage::{AttachmentCommunityAssignmentStore, AttachmentStore},
};

use super::{
    PostgresStorage,
    helpers::{map_unique, rows_to_page, run_count_query, run_dynamic_query, vec_to_page},
};

impl PostgresStorage {
    pub(super) fn query_attachments(
        connection: &mut PgConnection,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let rows = sql_query(
            "SELECT a.id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    a.mac_address,
                    a.comment,
                    a.created_at,
                    a.updated_at
             FROM host_attachments a
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             ORDER BY h.name, n.network, a.mac_address NULLS LAST",
        )
        .load::<HostAttachmentRow>(connection)?;
        rows.into_iter()
            .map(HostAttachmentRow::into_domain)
            .collect()
    }

    pub(super) fn query_attachment_by_id(
        connection: &mut PgConnection,
        attachment_id: Uuid,
    ) -> Result<HostAttachment, AppError> {
        sql_query(
            "SELECT a.id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    a.mac_address,
                    a.comment,
                    a.created_at,
                    a.updated_at
             FROM host_attachments a
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             WHERE a.id = $1",
        )
        .bind::<SqlUuid, _>(attachment_id)
        .get_result::<HostAttachmentRow>(connection)
        .optional()?
        .ok_or_else(|| AppError::not_found("host attachment was not found"))?
        .into_domain()
    }

    pub(super) fn find_or_create_attachment(
        connection: &mut PgConnection,
        host_name: &Hostname,
        network: &CidrValue,
        mac_address: Option<&MacAddressValue>,
    ) -> Result<HostAttachment, AppError> {
        let host_id = hosts::table
            .filter(hosts::name.eq(host_name.as_str()))
            .select(hosts::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!("host '{}' was not found", host_name.as_str()))
            })?;
        let network_obj = Self::query_network_by_cidr(connection, network)?;
        let maybe_existing = if let Some(mac_address) = mac_address {
            sql_query(
                "SELECT a.id,
                        a.host_id,
                        h.name::text AS host_name,
                        a.network_id,
                        n.network::text AS network_cidr,
                        a.mac_address,
                        a.comment,
                        a.created_at,
                        a.updated_at
                 FROM host_attachments a
                 JOIN hosts h ON h.id = a.host_id
                 JOIN networks n ON n.id = a.network_id
                 WHERE a.host_id = $1 AND a.network_id = $2 AND a.mac_address = $3",
            )
            .bind::<SqlUuid, _>(host_id)
            .bind::<SqlUuid, _>(network_obj.id())
            .bind::<Text, _>(mac_address.as_str())
            .get_result::<HostAttachmentRow>(connection)
            .optional()?
        } else {
            sql_query(
                "SELECT a.id,
                        a.host_id,
                        h.name::text AS host_name,
                        a.network_id,
                        n.network::text AS network_cidr,
                        a.mac_address,
                        a.comment,
                        a.created_at,
                        a.updated_at
                 FROM host_attachments a
                 JOIN hosts h ON h.id = a.host_id
                 JOIN networks n ON n.id = a.network_id
                 WHERE a.host_id = $1 AND a.network_id = $2 AND a.mac_address IS NULL
                 ORDER BY a.created_at
                 LIMIT 1",
            )
            .bind::<SqlUuid, _>(host_id)
            .bind::<SqlUuid, _>(network_obj.id())
            .get_result::<HostAttachmentRow>(connection)
            .optional()?
        };
        if let Some(existing) = maybe_existing {
            return HostAttachmentRow::into_domain(existing);
        }

        let row = sql_query(
            "INSERT INTO host_attachments (host_id, network_id, mac_address)
             VALUES ($1, $2, $3)
             RETURNING id,
                       host_id,
                       $4::text AS host_name,
                       network_id,
                       $5::text AS network_cidr,
                       mac_address,
                       comment,
                       created_at,
                       updated_at",
        )
        .bind::<SqlUuid, _>(host_id)
        .bind::<SqlUuid, _>(network_obj.id())
        .bind::<Nullable<Text>, _>(mac_address.map(|value| value.as_str()))
        .bind::<Text, _>(host_name.as_str())
        .bind::<Text, _>(network_obj.cidr().as_str())
        .get_result::<HostAttachmentRow>(connection)
        .map_err(map_unique("host attachment already exists"))?;
        row.into_domain()
    }

    pub(super) fn create_attachment_tx(
        connection: &mut PgConnection,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        let host_id = hosts::table
            .filter(hosts::name.eq(command.host_name().as_str()))
            .select(hosts::id)
            .first::<Uuid>(connection)
            .optional()?
            .ok_or_else(|| {
                AppError::not_found(format!(
                    "host '{}' was not found",
                    command.host_name().as_str()
                ))
            })?;
        let network = Self::query_network_by_cidr(connection, command.network())?;
        let existing = if let Some(mac_address) = command.mac_address() {
            sql_query(
                "SELECT a.id, ''::text AS name
                 FROM host_attachments a
                 WHERE a.host_id = $1 AND a.network_id = $2 AND a.mac_address = $3",
            )
            .bind::<SqlUuid, _>(host_id)
            .bind::<SqlUuid, _>(network.id())
            .bind::<Text, _>(mac_address.as_str())
            .get_result::<super::helpers::NameAndIdRow>(connection)
            .optional()?
            .map(|row| row.id)
        } else {
            sql_query(
                "SELECT a.id, ''::text AS name
                 FROM host_attachments a
                 WHERE a.host_id = $1 AND a.network_id = $2 AND a.mac_address IS NULL
                 ORDER BY a.created_at
                 LIMIT 1",
            )
            .bind::<SqlUuid, _>(host_id)
            .bind::<SqlUuid, _>(network.id())
            .get_result::<super::helpers::NameAndIdRow>(connection)
            .optional()?
            .map(|row| row.id)
        };
        if existing.is_some() {
            return Err(AppError::conflict("host attachment already exists"));
        }

        let row = sql_query(
            "INSERT INTO host_attachments (host_id, network_id, mac_address, comment)
             VALUES ($1, $2, $3, $4)
             RETURNING id,
                       host_id,
                       $5::text AS host_name,
                       network_id,
                       $6::text AS network_cidr,
                       mac_address,
                       comment,
                       created_at,
                       updated_at",
        )
        .bind::<SqlUuid, _>(host_id)
        .bind::<SqlUuid, _>(network.id())
        .bind::<Nullable<Text>, _>(command.mac_address().map(|value| value.as_str()))
        .bind::<Nullable<Text>, _>(command.comment())
        .bind::<Text, _>(command.host_name().as_str())
        .bind::<Text, _>(network.cidr().as_str())
        .get_result::<HostAttachmentRow>(connection)
        .map_err(map_unique("host attachment already exists"))?;

        row.into_domain()
    }

    pub(super) fn list_attachment_dhcp_identifiers_tx(
        connection: &mut PgConnection,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        let rows = sql_query(
            "SELECT id, attachment_id, family::int AS family, kind, value, priority, created_at, updated_at
             FROM attachment_dhcp_identifiers
             WHERE attachment_id = $1
             ORDER BY family, priority, kind, value",
        )
        .bind::<SqlUuid, _>(attachment_id)
        .load::<AttachmentDhcpIdentifierRow>(connection)?;
        rows.into_iter()
            .map(AttachmentDhcpIdentifierRow::into_domain)
            .collect()
    }

    pub(super) fn list_attachment_dhcp_identifiers_for_attachments_tx(
        connection: &mut PgConnection,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        if attachment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sql_query(
            "SELECT id, attachment_id, family::int AS family, kind, value, priority, created_at, updated_at
             FROM attachment_dhcp_identifiers
             WHERE attachment_id = ANY($1)
             ORDER BY attachment_id, family, priority, kind, value",
        )
        .bind::<Array<SqlUuid>, _>(attachment_ids)
        .load::<AttachmentDhcpIdentifierRow>(connection)?;
        rows.into_iter()
            .map(AttachmentDhcpIdentifierRow::into_domain)
            .collect()
    }

    pub(super) fn query_all_dhcp_identifiers(
        connection: &mut PgConnection,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        let rows = sql_query(
            "SELECT id, attachment_id, family::int AS family, kind, value, priority, created_at, updated_at
             FROM attachment_dhcp_identifiers
             ORDER BY attachment_id, family, priority, kind, value",
        )
        .load::<AttachmentDhcpIdentifierRow>(connection)?;
        rows.into_iter()
            .map(AttachmentDhcpIdentifierRow::into_domain)
            .collect()
    }

    pub(super) fn create_attachment_dhcp_identifier_tx(
        connection: &mut PgConnection,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        Self::query_attachment_by_id(connection, command.attachment_id())?;
        sql_query(
            "INSERT INTO attachment_dhcp_identifiers (attachment_id, family, kind, value, priority)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, attachment_id, family::int AS family, kind, value, priority, created_at, updated_at",
        )
        .bind::<SqlUuid, _>(command.attachment_id())
        .bind::<Integer, _>(command.family().as_u8() as i32)
        .bind::<Text, _>(match command.kind() {
            DhcpIdentifierKind::ClientId => "client_id",
            DhcpIdentifierKind::DuidLlt => "duid_llt",
            DhcpIdentifierKind::DuidEn => "duid_en",
            DhcpIdentifierKind::DuidLl => "duid_ll",
            DhcpIdentifierKind::DuidUuid => "duid_uuid",
            DhcpIdentifierKind::DuidRaw => "duid_raw",
        })
        .bind::<Text, _>(command.value())
        .bind::<Integer, _>(command.priority().as_i32())
        .get_result::<AttachmentDhcpIdentifierRow>(connection)
        .map_err(map_unique("attachment DHCP identifier already exists"))?
        .into_domain()
    }

    pub(super) fn list_attachment_prefix_reservations_tx(
        connection: &mut PgConnection,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        let rows = sql_query(
            "SELECT id, attachment_id, prefix::text AS prefix, created_at, updated_at
             FROM attachment_prefix_reservations
             WHERE attachment_id = $1
             ORDER BY prefix",
        )
        .bind::<SqlUuid, _>(attachment_id)
        .load::<AttachmentPrefixReservationRow>(connection)?;
        rows.into_iter()
            .map(AttachmentPrefixReservationRow::into_domain)
            .collect()
    }

    pub(super) fn list_attachment_prefix_reservations_for_attachments_tx(
        connection: &mut PgConnection,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        if attachment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sql_query(
            "SELECT id, attachment_id, prefix::text AS prefix, created_at, updated_at
             FROM attachment_prefix_reservations
             WHERE attachment_id = ANY($1)
             ORDER BY attachment_id, prefix",
        )
        .bind::<Array<SqlUuid>, _>(attachment_ids)
        .load::<AttachmentPrefixReservationRow>(connection)?;
        rows.into_iter()
            .map(AttachmentPrefixReservationRow::into_domain)
            .collect()
    }

    pub(super) fn list_attachment_community_assignments_for_attachments_tx(
        connection: &mut PgConnection,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        if attachment_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sql_query(
            "SELECT aca.id,
                    aca.attachment_id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    c.id AS community_id,
                    c.name::text AS community_name,
                    np.name::text AS policy_name,
                    aca.created_at,
                    aca.updated_at
             FROM attachment_community_assignments aca
             JOIN host_attachments a ON a.id = aca.attachment_id
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             JOIN communities c ON c.id = aca.community_id
             JOIN network_policies np ON np.id = c.policy_id
             WHERE aca.attachment_id = ANY($1)
             ORDER BY aca.attachment_id, c.name",
        )
        .bind::<Array<SqlUuid>, _>(attachment_ids)
        .load::<AttachmentCommunityAssignmentRow>(connection)?;
        rows.into_iter()
            .map(AttachmentCommunityAssignmentRow::into_domain)
            .collect()
    }

    pub(super) fn query_all_prefix_reservations(
        connection: &mut PgConnection,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        let rows = sql_query(
            "SELECT id, attachment_id, prefix::text AS prefix, created_at, updated_at
             FROM attachment_prefix_reservations
             ORDER BY attachment_id, prefix",
        )
        .load::<AttachmentPrefixReservationRow>(connection)?;
        rows.into_iter()
            .map(AttachmentPrefixReservationRow::into_domain)
            .collect()
    }

    pub(super) fn create_attachment_prefix_reservation_tx(
        connection: &mut PgConnection,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        let attachment = Self::query_attachment_by_id(connection, command.attachment_id())?;
        validate_prefix_reservation_for_attachment(&attachment, command.prefix())?;
        sql_query(
            "INSERT INTO attachment_prefix_reservations (attachment_id, prefix)
             VALUES ($1, $2::cidr)
             RETURNING id, attachment_id, prefix::text AS prefix, created_at, updated_at",
        )
        .bind::<SqlUuid, _>(command.attachment_id())
        .bind::<Text, _>(command.prefix().as_str())
        .get_result::<AttachmentPrefixReservationRow>(connection)
        .map_err(map_unique("attachment prefix reservation already exists"))?
        .into_domain()
    }

    pub(super) fn create_attachment_community_assignment_tx(
        connection: &mut PgConnection,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        let attachment = Self::query_attachment_by_id(connection, command.attachment_id())?;
        let community = super::communities::find_by_names(
            connection,
            command.policy_name().as_str(),
            command.community_name().as_str(),
        )?;
        if community.network_cidr() != attachment.network_cidr() {
            return Err(AppError::not_found(format!(
                "community '{}/{}' was not found for attachment network",
                command.policy_name().as_str(),
                command.community_name().as_str()
            )));
        }
        let row = sql_query(
            "INSERT INTO attachment_community_assignments (attachment_id, community_id)
             VALUES ($1, $2)
             RETURNING id,
                       attachment_id,
                       $3::uuid AS host_id,
                       $4::text AS host_name,
                       $5::uuid AS network_id,
                       $6::text AS network_cidr,
                       $7::uuid AS community_id,
                       $8::text AS community_name,
                       $9::text AS policy_name,
                       created_at,
                       updated_at",
        )
        .bind::<SqlUuid, _>(attachment.id())
        .bind::<SqlUuid, _>(community.id())
        .bind::<SqlUuid, _>(attachment.host_id())
        .bind::<Text, _>(attachment.host_name().as_str())
        .bind::<SqlUuid, _>(attachment.network_id())
        .bind::<Text, _>(attachment.network_cidr().as_str())
        .bind::<SqlUuid, _>(community.id())
        .bind::<Text, _>(community.name().as_str())
        .bind::<Text, _>(community.policy_name().as_str())
        .get_result::<AttachmentCommunityAssignmentRow>(connection)
        .map_err(map_unique("attachment community assignment already exists"))?;
        row.into_domain()
    }
}

#[async_trait]
impl AttachmentStore for PostgresStorage {
    async fn list_attachments(&self, page: &PageRequest) -> Result<Page<HostAttachment>, AppError> {
        let page = page.clone();
        self.database
            .run(move |connection| {
                let items = Self::query_attachments(connection)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn list_attachments_for_host(
        &self,
        host: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let host = host.clone();
        self.database
            .run(move |connection| {
                let rows = sql_query(
                    "SELECT a.id,
                            a.host_id,
                            h.name::text AS host_name,
                            a.network_id,
                            n.network::text AS network_cidr,
                            a.mac_address,
                            a.comment,
                            a.created_at,
                            a.updated_at
                     FROM host_attachments a
                     JOIN hosts h ON h.id = a.host_id
                     JOIN networks n ON n.id = a.network_id
                     WHERE h.name = $1
                     ORDER BY n.network, a.mac_address NULLS LAST",
                )
                .bind::<Text, _>(host.as_str())
                .load::<HostAttachmentRow>(connection)?;
                rows.into_iter()
                    .map(HostAttachmentRow::into_domain)
                    .collect()
            })
            .await
    }

    async fn list_attachments_for_hosts(
        &self,
        hosts: &[Hostname],
    ) -> Result<Vec<HostAttachment>, AppError> {
        let hosts = hosts
            .iter()
            .map(|host| host.as_str().to_string())
            .collect::<Vec<_>>();
        self.database
            .run(move |connection| {
                if hosts.is_empty() {
                    return Ok(Vec::new());
                }
                let rows = sql_query(
                    "SELECT a.id,
                            a.host_id,
                            h.name::text AS host_name,
                            a.network_id,
                            n.network::text AS network_cidr,
                            a.mac_address,
                            a.comment,
                            a.created_at,
                            a.updated_at
                     FROM host_attachments a
                     JOIN hosts h ON h.id = a.host_id
                     JOIN networks n ON n.id = a.network_id
                     WHERE h.name = ANY($1::text[])
                     ORDER BY h.name, n.network, a.mac_address NULLS LAST",
                )
                .bind::<Array<Text>, _>(&hosts)
                .load::<HostAttachmentRow>(connection)?;
                rows.into_iter()
                    .map(HostAttachmentRow::into_domain)
                    .collect()
            })
            .await
    }

    async fn list_attachments_for_network(
        &self,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let network = network.clone();
        self.database
            .run(move |connection| {
                let rows = sql_query(
                    "SELECT a.id,
                            a.host_id,
                            h.name::text AS host_name,
                            a.network_id,
                            n.network::text AS network_cidr,
                            a.mac_address,
                            a.comment,
                            a.created_at,
                            a.updated_at
                     FROM host_attachments a
                     JOIN hosts h ON h.id = a.host_id
                     JOIN networks n ON n.id = a.network_id
                     WHERE $1::cidr >>= n.network
                     ORDER BY h.name, a.mac_address NULLS LAST",
                )
                .bind::<Text, _>(network.as_str())
                .load::<HostAttachmentRow>(connection)?;
                rows.into_iter()
                    .map(HostAttachmentRow::into_domain)
                    .collect()
            })
            .await
    }

    async fn create_attachment(
        &self,
        command: CreateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        self.database
            .run(move |connection| Self::create_attachment_tx(connection, command))
            .await
    }

    async fn get_attachment(&self, attachment_id: Uuid) -> Result<HostAttachment, AppError> {
        self.database
            .run(move |connection| Self::query_attachment_by_id(connection, attachment_id))
            .await
    }

    async fn update_attachment(
        &self,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        self.database
            .run(move |connection| Self::update_attachment_tx(connection, attachment_id, command))
            .await
    }

    async fn delete_attachment(&self, attachment_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                let ip_count = ip_addresses::table
                    .filter(ip_addresses::attachment_id.eq(attachment_id))
                    .count()
                    .get_result::<i64>(connection)?;
                if ip_count > 0 {
                    return Err(AppError::conflict(
                        "host attachment still owns IP address reservations",
                    ));
                }
                let deleted = diesel::delete(
                    host_attachments::table.filter(host_attachments::id.eq(attachment_id)),
                )
                .execute(connection)?;
                if deleted == 0 {
                    return Err(AppError::not_found("host attachment was not found"));
                }
                Ok(())
            })
            .await
    }

    async fn list_attachment_dhcp_identifiers(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        self.database
            .run(move |connection| {
                Self::list_attachment_dhcp_identifiers_tx(connection, attachment_id)
            })
            .await
    }

    async fn list_attachment_dhcp_identifiers_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentDhcpIdentifier>, AppError> {
        let attachment_ids = attachment_ids.to_vec();
        self.database
            .run(move |connection| {
                Self::list_attachment_dhcp_identifiers_for_attachments_tx(
                    connection,
                    &attachment_ids,
                )
            })
            .await
    }

    async fn create_attachment_dhcp_identifier(
        &self,
        command: CreateAttachmentDhcpIdentifier,
    ) -> Result<AttachmentDhcpIdentifier, AppError> {
        self.database
            .run(move |connection| Self::create_attachment_dhcp_identifier_tx(connection, command))
            .await
    }

    async fn delete_attachment_dhcp_identifier(&self, identifier_id: Uuid) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                let deleted = diesel::delete(
                    attachment_dhcp_identifiers::table
                        .filter(attachment_dhcp_identifiers::id.eq(identifier_id)),
                )
                .execute(connection)?;
                if deleted == 0 {
                    return Err(AppError::not_found(
                        "attachment DHCP identifier was not found",
                    ));
                }
                Ok(())
            })
            .await
    }

    async fn list_attachment_prefix_reservations(
        &self,
        attachment_id: Uuid,
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        self.database
            .run(move |connection| {
                Self::list_attachment_prefix_reservations_tx(connection, attachment_id)
            })
            .await
    }

    async fn list_attachment_prefix_reservations_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentPrefixReservation>, AppError> {
        let attachment_ids = attachment_ids.to_vec();
        self.database
            .run(move |connection| {
                Self::list_attachment_prefix_reservations_for_attachments_tx(
                    connection,
                    &attachment_ids,
                )
            })
            .await
    }

    async fn create_attachment_prefix_reservation(
        &self,
        command: CreateAttachmentPrefixReservation,
    ) -> Result<AttachmentPrefixReservation, AppError> {
        self.database
            .run(move |connection| {
                Self::create_attachment_prefix_reservation_tx(connection, command)
            })
            .await
    }

    async fn delete_attachment_prefix_reservation(
        &self,
        reservation_id: Uuid,
    ) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                let deleted = diesel::delete(
                    attachment_prefix_reservations::table
                        .filter(attachment_prefix_reservations::id.eq(reservation_id)),
                )
                .execute(connection)?;
                if deleted == 0 {
                    return Err(AppError::not_found(
                        "attachment prefix reservation was not found",
                    ));
                }
                Ok(())
            })
            .await
    }
}

impl PostgresStorage {
    pub(in crate::storage::postgres) fn list_attachments_in_conn(
        connection: &mut PgConnection,
        page: &PageRequest,
    ) -> Result<Page<HostAttachment>, AppError> {
        let items = Self::query_attachments(connection)?;
        Ok(vec_to_page(items, page))
    }

    pub(in crate::storage::postgres) fn list_attachments_for_host_in_conn(
        connection: &mut PgConnection,
        host: &Hostname,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let rows = sql_query(
            "SELECT a.id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    a.mac_address,
                    a.comment,
                    a.created_at,
                    a.updated_at
             FROM host_attachments a
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             WHERE h.name = $1
             ORDER BY n.network, a.mac_address NULLS LAST",
        )
        .bind::<Text, _>(host.as_str())
        .load::<HostAttachmentRow>(connection)?;
        rows.into_iter()
            .map(HostAttachmentRow::into_domain)
            .collect()
    }

    pub(in crate::storage::postgres) fn list_attachments_for_hosts_in_conn(
        connection: &mut PgConnection,
        hosts: &[Hostname],
    ) -> Result<Vec<HostAttachment>, AppError> {
        if hosts.is_empty() {
            return Ok(Vec::new());
        }
        let host_names = hosts
            .iter()
            .map(|host| host.as_str().to_string())
            .collect::<Vec<_>>();
        let rows = sql_query(
            "SELECT a.id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    a.mac_address,
                    a.comment,
                    a.created_at,
                    a.updated_at
             FROM host_attachments a
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             WHERE h.name = ANY($1::text[])
             ORDER BY h.name, n.network, a.mac_address NULLS LAST",
        )
        .bind::<Array<Text>, _>(&host_names)
        .load::<HostAttachmentRow>(connection)?;
        rows.into_iter()
            .map(HostAttachmentRow::into_domain)
            .collect()
    }

    pub(in crate::storage::postgres) fn list_attachments_for_network_in_conn(
        connection: &mut PgConnection,
        network: &CidrValue,
    ) -> Result<Vec<HostAttachment>, AppError> {
        let rows = sql_query(
            "SELECT a.id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    a.mac_address,
                    a.comment,
                    a.created_at,
                    a.updated_at
             FROM host_attachments a
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             WHERE $1::cidr >>= n.network
             ORDER BY h.name, a.mac_address NULLS LAST",
        )
        .bind::<Text, _>(network.as_str())
        .load::<HostAttachmentRow>(connection)?;
        rows.into_iter()
            .map(HostAttachmentRow::into_domain)
            .collect()
    }

    pub(in crate::storage::postgres) fn delete_attachment_in_conn(
        connection: &mut PgConnection,
        attachment_id: Uuid,
    ) -> Result<(), AppError> {
        let ip_count = ip_addresses::table
            .filter(ip_addresses::attachment_id.eq(attachment_id))
            .count()
            .get_result::<i64>(connection)?;
        if ip_count > 0 {
            return Err(AppError::conflict(
                "host attachment still owns IP address reservations",
            ));
        }
        let deleted = diesel::delete(
            host_attachments::table.filter(host_attachments::id.eq(attachment_id)),
        )
        .execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found("host attachment was not found"));
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn delete_attachment_dhcp_identifier_in_conn(
        connection: &mut PgConnection,
        identifier_id: Uuid,
    ) -> Result<(), AppError> {
        let deleted = diesel::delete(
            attachment_dhcp_identifiers::table
                .filter(attachment_dhcp_identifiers::id.eq(identifier_id)),
        )
        .execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found(
                "attachment DHCP identifier was not found",
            ));
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn delete_attachment_prefix_reservation_in_conn(
        connection: &mut PgConnection,
        reservation_id: Uuid,
    ) -> Result<(), AppError> {
        let deleted = diesel::delete(
            attachment_prefix_reservations::table
                .filter(attachment_prefix_reservations::id.eq(reservation_id)),
        )
        .execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found(
                "attachment prefix reservation was not found",
            ));
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn list_attachment_community_assignments_in_conn(
        connection: &mut PgConnection,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        let (clauses, values) = filter.sql_conditions();
        let mut query = String::from(
            "SELECT aca.id,
                    aca.attachment_id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    c.id AS community_id,
                    c.name::text AS community_name,
                    np.name::text AS policy_name,
                    aca.created_at,
                    aca.updated_at
             FROM attachment_community_assignments aca
             JOIN host_attachments a ON a.id = aca.attachment_id
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             JOIN communities c ON c.id = aca.community_id
             JOIN network_policies np ON np.id = c.policy_id",
        );
        if !clauses.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&clauses.join(" AND "));
        }
        let order_col = match page.sort_by() {
            Some("network") => "n.network::text",
            Some("policy_name") => "np.name::text",
            Some("community_name") => "c.name::text",
            None => "h.name::text",
            Some(other) => {
                return Err(AppError::validation(format!(
                    "unsupported sort_by field for attachments: {other}"
                )));
            }
        };
        let order_dir = match page.sort_direction() {
            crate::domain::pagination::SortDirection::Asc => "ASC",
            crate::domain::pagination::SortDirection::Desc => "DESC",
        };
        let count_sql = format!("SELECT COUNT(*) AS count FROM ({query}) AS _c");
        let total = run_count_query(connection, &count_sql, &values)?;

        let limit_clause = if page.after().is_none() && page.limit() != u64::MAX {
            format!(" LIMIT {}", page.limit() + 1)
        } else {
            String::new()
        };
        query.push_str(&format!(
            " ORDER BY {order_col} {order_dir}, aca.id{limit_clause}"
        ));

        let rows = run_dynamic_query::<AttachmentCommunityAssignmentRow>(
            connection, &query, &values,
        )?;
        let items = rows
            .into_iter()
            .map(AttachmentCommunityAssignmentRow::into_domain)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows_to_page(items, page, total))
    }

    pub(in crate::storage::postgres) fn get_attachment_community_assignment_in_conn(
        connection: &mut PgConnection,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        sql_query(
            "SELECT aca.id,
                    aca.attachment_id,
                    a.host_id,
                    h.name::text AS host_name,
                    a.network_id,
                    n.network::text AS network_cidr,
                    c.id AS community_id,
                    c.name::text AS community_name,
                    np.name::text AS policy_name,
                    aca.created_at,
                    aca.updated_at
             FROM attachment_community_assignments aca
             JOIN host_attachments a ON a.id = aca.attachment_id
             JOIN hosts h ON h.id = a.host_id
             JOIN networks n ON n.id = a.network_id
             JOIN communities c ON c.id = aca.community_id
             JOIN network_policies np ON np.id = c.policy_id
             WHERE aca.id = $1",
        )
        .bind::<SqlUuid, _>(assignment_id)
        .get_result::<AttachmentCommunityAssignmentRow>(connection)
        .optional()?
        .ok_or_else(|| AppError::not_found("attachment community assignment was not found"))?
        .into_domain()
    }

    pub(in crate::storage::postgres) fn delete_attachment_community_assignment_in_conn(
        connection: &mut PgConnection,
        assignment_id: Uuid,
    ) -> Result<(), AppError> {
        let deleted = diesel::delete(
            attachment_community_assignments::table
                .filter(attachment_community_assignments::id.eq(assignment_id)),
        )
        .execute(connection)?;
        if deleted == 0 {
            return Err(AppError::not_found(
                "attachment community assignment was not found",
            ));
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn update_attachment_tx(
        connection: &mut PgConnection,
        attachment_id: Uuid,
        command: UpdateHostAttachment,
    ) -> Result<HostAttachment, AppError> {
        let old = Self::query_attachment_by_id(connection, attachment_id)?;
        let mac_address: Option<String> = command
            .mac_address
            .map(|mac| mac.as_str().to_string())
            .resolve(old.mac_address().map(|mac| mac.as_str().to_string()));
        let comment: Option<String> = command.comment.resolve(old.comment().map(str::to_string));
        sql_query(
            "UPDATE host_attachments
             SET mac_address = $1, comment = $2, updated_at = now()
             WHERE id = $3
             RETURNING id,
                       host_id,
                       $4::text AS host_name,
                       network_id,
                       $5::text AS network_cidr,
                       mac_address,
                       comment,
                       created_at,
                       updated_at",
        )
        .bind::<Nullable<Text>, _>(mac_address.as_deref())
        .bind::<Nullable<Text>, _>(comment.as_deref())
        .bind::<SqlUuid, _>(attachment_id)
        .bind::<Text, _>(old.host_name().as_str())
        .bind::<Text, _>(old.network_cidr().as_str())
        .get_result::<HostAttachmentRow>(connection)
        .map_err(map_unique("host attachment already exists"))?
        .into_domain()
    }
}

#[async_trait]
impl AttachmentCommunityAssignmentStore for PostgresStorage {
    async fn list_attachment_community_assignments(
        &self,
        page: &PageRequest,
        filter: &AttachmentCommunityAssignmentFilter,
    ) -> Result<Page<AttachmentCommunityAssignment>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |connection| {
                let (clauses, values) = filter.sql_conditions();
                let mut query = String::from(
                    "SELECT aca.id,
                            aca.attachment_id,
                            a.host_id,
                            h.name::text AS host_name,
                            a.network_id,
                            n.network::text AS network_cidr,
                            c.id AS community_id,
                            c.name::text AS community_name,
                            np.name::text AS policy_name,
                            aca.created_at,
                            aca.updated_at
                     FROM attachment_community_assignments aca
                     JOIN host_attachments a ON a.id = aca.attachment_id
                     JOIN hosts h ON h.id = a.host_id
                     JOIN networks n ON n.id = a.network_id
                     JOIN communities c ON c.id = aca.community_id
                     JOIN network_policies np ON np.id = c.policy_id",
                );
                if !clauses.is_empty() {
                    query.push_str(" WHERE ");
                    query.push_str(&clauses.join(" AND "));
                }
                let order_col = match page.sort_by() {
                    Some("network") => "n.network::text",
                    Some("policy_name") => "np.name::text",
                    Some("community_name") => "c.name::text",
                    None => "h.name::text",
                    Some(other) => {
                        return Err(AppError::validation(format!(
                            "unsupported sort_by field for attachments: {other}"
                        )));
                    }
                };
                let order_dir = match page.sort_direction() {
                    crate::domain::pagination::SortDirection::Asc => "ASC",
                    crate::domain::pagination::SortDirection::Desc => "DESC",
                };
                let count_sql = format!("SELECT COUNT(*) AS count FROM ({query}) AS _c");
                let total = run_count_query(connection, &count_sql, &values)?;

                let limit_clause = if page.after().is_none() && page.limit() != u64::MAX {
                    format!(" LIMIT {}", page.limit() + 1)
                } else {
                    String::new()
                };
                query.push_str(&format!(
                    " ORDER BY {order_col} {order_dir}, aca.id{limit_clause}"
                ));

                let rows = run_dynamic_query::<AttachmentCommunityAssignmentRow>(
                    connection, &query, &values,
                )?;
                let items = rows
                    .into_iter()
                    .map(AttachmentCommunityAssignmentRow::into_domain)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows_to_page(items, &page, total))
            })
            .await
    }

    async fn list_attachment_community_assignments_for_attachments(
        &self,
        attachment_ids: &[Uuid],
    ) -> Result<Vec<AttachmentCommunityAssignment>, AppError> {
        let attachment_ids = attachment_ids.to_vec();
        self.database
            .run(move |connection| {
                Self::list_attachment_community_assignments_for_attachments_tx(
                    connection,
                    &attachment_ids,
                )
            })
            .await
    }

    async fn create_attachment_community_assignment(
        &self,
        command: CreateAttachmentCommunityAssignment,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        self.database
            .run(move |connection| {
                Self::create_attachment_community_assignment_tx(connection, command)
            })
            .await
    }

    async fn get_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<AttachmentCommunityAssignment, AppError> {
        self.database
            .run(move |connection| {
                sql_query(
                    "SELECT aca.id,
                            aca.attachment_id,
                            a.host_id,
                            h.name::text AS host_name,
                            a.network_id,
                            n.network::text AS network_cidr,
                            c.id AS community_id,
                            c.name::text AS community_name,
                            np.name::text AS policy_name,
                            aca.created_at,
                            aca.updated_at
                     FROM attachment_community_assignments aca
                     JOIN host_attachments a ON a.id = aca.attachment_id
                     JOIN hosts h ON h.id = a.host_id
                     JOIN networks n ON n.id = a.network_id
                     JOIN communities c ON c.id = aca.community_id
                     JOIN network_policies np ON np.id = c.policy_id
                     WHERE aca.id = $1",
                )
                .bind::<SqlUuid, _>(assignment_id)
                .get_result::<AttachmentCommunityAssignmentRow>(connection)
                .optional()?
                .ok_or_else(|| {
                    AppError::not_found("attachment community assignment was not found")
                })?
                .into_domain()
            })
            .await
    }

    async fn delete_attachment_community_assignment(
        &self,
        assignment_id: Uuid,
    ) -> Result<(), AppError> {
        self.database
            .run(move |connection| {
                let deleted = diesel::delete(
                    attachment_community_assignments::table
                        .filter(attachment_community_assignments::id.eq(assignment_id)),
                )
                .execute(connection)?;
                if deleted == 0 {
                    return Err(AppError::not_found(
                        "attachment community assignment was not found",
                    ));
                }
                Ok(())
            })
            .await
    }
}
