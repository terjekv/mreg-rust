use std::collections::BTreeMap;

use async_trait::async_trait;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, PgConnection, QueryDsl, RunQueryDsl,
    SelectableHelper, sql_query,
    sql_types::{Bool, Integer, Nullable, Text, Uuid as SqlUuid},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    db::models::{ExcludedRangeRow, HostRow, ImportBatchRow, NameServerRow, NetworkRow, UuidRow},
    domain::{
        attachment::{
            CreateAttachmentCommunityAssignment, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, DhcpIdentifierFamily,
            DhcpIdentifierKind,
        },
        bacnet::CreateBacnetIdAssignment,
        community::CreateCommunity,
        host::{AssignIpAddress, CreateHost},
        host_community_assignment::CreateHostCommunityAssignment,
        host_contact::CreateHostContact,
        host_group::CreateHostGroup,
        host_policy::{CreateHostPolicyAtom, CreateHostPolicyRole},
        imports::{CreateImportBatch, ImportBatchSummary, ImportItem, ImportKind, ImportOperation},
        network::{CreateExcludedRange, CreateNetwork},
        network_policy::CreateNetworkPolicy,
        pagination::{Page, PageRequest},
        ptr_override::CreatePtrOverride,
        resource_records::{
            CreateRecordInstance, RawRdataValue, ValidatedRecordContent, alias_target_names,
            validate_record_relationships,
        },
        tasks::CreateTask,
        types::{
            BacnetIdentifier, CidrValue, CommunityName, DhcpPriority, DnsName, EmailAddressValue,
            HostGroupName, HostPolicyName, Hostname, IpAddressValue, LabelName, MacAddressValue,
            NetworkPolicyName, OwnerGroupName, RecordTypeName, ReservedCount, SerialNumber,
            SoaSeconds, Ttl, VlanId, ZoneName,
        },
        zone::{
            CreateForwardZone, CreateForwardZoneDelegation, CreateReverseZone,
            CreateReverseZoneDelegation,
        },
    },
    errors::AppError,
    storage::ImportStore,
    storage::import_helpers::{
        resolve_bool, resolve_i32, resolve_one_of_string, resolve_optional_string,
        resolve_required_one_of_string, resolve_string, resolve_string_vec, resolve_u32,
        resolve_u64, resolve_uuid, stringify_ref_value,
    },
};

use super::PostgresStorage;
use super::helpers::{map_unique, vec_to_page};

use crate::db::models::{ForwardZoneRow, LabelRow, ReverseZoneRow};
use crate::db::schema::labels;

impl PostgresStorage {
    fn query_import_summaries(
        connection: &mut PgConnection,
    ) -> Result<Vec<ImportBatchSummary>, AppError> {
        use crate::db::schema::imports;
        use diesel::QueryDsl;

        let rows = imports::table
            .select(ImportBatchRow::as_select())
            .order(imports::created_at.desc())
            .load::<ImportBatchRow>(connection)?;
        rows.into_iter().map(ImportBatchRow::into_summary).collect()
    }

    fn create_import_item(
        connection: &mut PgConnection,
        item: &ImportItem,
        refs: &mut BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        match item.operation() {
            ImportOperation::Create => {}
        }

        let attributes = item.attributes();
        let result = match item.kind() {
            ImportKind::Label => Self::import_label(connection, attributes, refs)?,
            ImportKind::Nameserver => Self::import_nameserver(connection, attributes, refs)?,
            ImportKind::Network => Self::import_network(connection, attributes, refs)?,
            ImportKind::HostContact => Self::import_host_contact(connection, attributes, refs)?,
            ImportKind::HostGroup => Self::import_host_group(connection, attributes, refs)?,
            ImportKind::BacnetId => Self::import_bacnet_id(connection, attributes, refs)?,
            ImportKind::PtrOverride => Self::import_ptr_override(connection, attributes, refs)?,
            ImportKind::NetworkPolicy => Self::import_network_policy(connection, attributes, refs)?,
            ImportKind::NetworkPolicyAttribute => {
                Self::import_network_policy_attribute(connection, attributes, refs)?
            }
            ImportKind::NetworkPolicyAttributeValue => {
                Self::import_network_policy_attribute_value(connection, attributes, refs)?
            }
            ImportKind::Community => Self::import_community(connection, attributes, refs)?,
            ImportKind::ForwardZone => Self::import_forward_zone(connection, attributes, refs)?,
            ImportKind::ReverseZone => Self::import_reverse_zone(connection, attributes, refs)?,
            ImportKind::ForwardZoneDelegation => {
                Self::import_forward_zone_delegation(connection, attributes, refs)?
            }
            ImportKind::ReverseZoneDelegation => {
                Self::import_reverse_zone_delegation(connection, attributes, refs)?
            }
            ImportKind::ExcludedRange => Self::import_excluded_range(connection, attributes, refs)?,
            ImportKind::Host => Self::import_host(connection, attributes, refs)?,
            ImportKind::HostAttachment => {
                Self::import_host_attachment(connection, attributes, refs)?
            }
            ImportKind::IpAddress => Self::import_ip_address(connection, attributes, refs)?,
            ImportKind::Record => Self::import_record(connection, attributes, refs)?,
            ImportKind::AttachmentDhcpIdentifier => {
                Self::import_attachment_dhcp_identifier(connection, attributes, refs)?
            }
            ImportKind::AttachmentPrefixReservation => {
                Self::import_attachment_prefix_reservation(connection, attributes, refs)?
            }
            ImportKind::AttachmentCommunityAssignment => {
                Self::import_attachment_community_assignment(connection, attributes, refs)?
            }
            ImportKind::HostCommunityAssignment => {
                Self::import_host_community_assignment(connection, attributes, refs)?
            }
            ImportKind::HostPolicyAtom => {
                Self::import_host_policy_atom(connection, attributes, refs)?
            }
            ImportKind::HostPolicyRole => {
                Self::import_host_policy_role(connection, attributes, refs)?
            }
            ImportKind::HostPolicyRoleAtom => {
                Self::import_host_policy_role_atom(connection, attributes, refs)?
            }
            ImportKind::HostPolicyRoleHost => {
                Self::import_host_policy_role_host(connection, attributes, refs)?
            }
            ImportKind::HostPolicyRoleLabel => {
                Self::import_host_policy_role_label(connection, attributes, refs)?
            }
        };

        refs.insert(item.reference().to_string(), stringify_ref_value(&result));
        Ok(json!({
            "ref": item.reference(),
            "kind": item.kind(),
            "result": result,
        }))
    }

    fn import_label(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let label_name = LabelName::new(resolve_string(attributes, "name", refs)?)?;
        let description = resolve_string(attributes, "description", refs)?;
        let label = diesel::insert_into(labels::table)
            .values((
                labels::name.eq(label_name.as_str()),
                labels::description.eq(&description),
            ))
            .returning(LabelRow::as_returning())
            .get_result(connection)
            .map_err(map_unique("label already exists"))?
            .into_domain()?;
        Ok(Value::String(label.name().as_str().to_string()))
    }

    fn import_nameserver(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        use crate::db::schema::nameservers;
        let ns_name = DnsName::new(resolve_string(attributes, "name", refs)?)?;
        let ttl = resolve_u32(attributes, "ttl")?.map(Ttl::new).transpose()?;
        let nameserver = diesel::insert_into(nameservers::table)
            .values((
                nameservers::name.eq(ns_name.as_str()),
                nameservers::ttl.eq(ttl.map(|value| value.as_i32())),
            ))
            .returning(NameServerRow::as_returning())
            .get_result(connection)
            .map_err(map_unique("nameserver already exists"))?
            .into_domain()?;
        Ok(Value::String(nameserver.name().as_str().to_string()))
    }

    fn import_network(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let cidr = CidrValue::new(resolve_string(attributes, "cidr", refs)?)?;
        let command = CreateNetwork::new_full(
            cidr.clone(),
            resolve_string(attributes, "description", refs)?,
            resolve_u32(attributes, "vlan")?
                .map(VlanId::new)
                .transpose()?,
            resolve_bool(attributes, "dns_delegated")?.unwrap_or(false),
            resolve_optional_string(attributes, "category", refs)?.unwrap_or_default(),
            resolve_optional_string(attributes, "location", refs)?.unwrap_or_default(),
            resolve_bool(attributes, "frozen")?.unwrap_or(false),
            ReservedCount::new(resolve_u32(attributes, "reserved")?.unwrap_or(3))?,
        )?;
        let policy_name = resolve_one_of_string(attributes, &["policy_name", "policy"], refs)?
            .map(NetworkPolicyName::new)
            .transpose()?;
        let policy_id = policy_name
            .as_ref()
            .map(|name| Self::resolve_network_policy_id(connection, name))
            .transpose()?;
        let max_communities = resolve_i32(attributes, "max_communities")?;
        let network = sql_query(
            "INSERT INTO networks
                (network, description, vlan, dns_delegated, category, location, frozen, reserved, max_communities, policy_id)
             VALUES
                ($1::cidr, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id, network::text AS network, description, vlan, dns_delegated,
                       category, location, frozen, reserved, created_at, updated_at",
        )
        .bind::<Text, _>(command.cidr().as_str())
        .bind::<Text, _>(command.description())
        .bind::<Nullable<Integer>, _>(command.vlan().map(|value| value.as_i32()))
        .bind::<Bool, _>(command.dns_delegated())
        .bind::<Text, _>(command.category())
        .bind::<Text, _>(command.location())
        .bind::<Bool, _>(command.frozen())
        .bind::<Integer, _>(command.reserved().as_i32())
        .bind::<Nullable<Integer>, _>(max_communities)
        .bind::<Nullable<SqlUuid>, _>(policy_id)
        .get_result::<NetworkRow>(connection)
        .map_err(map_unique("network already exists"))?
        .into_domain()?;
        Ok(Value::String(network.cidr().as_str()))
    }

    fn import_host_contact(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let contact = super::host_contacts::create(
            connection,
            CreateHostContact::new(
                EmailAddressValue::new(resolve_string(attributes, "email", refs)?)?,
                resolve_optional_string(attributes, "display_name", refs)?,
                resolve_string_vec(attributes, "hosts", refs)?
                    .into_iter()
                    .map(Hostname::new)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )?;
        Ok(Value::String(contact.email().as_str().to_string()))
    }

    fn import_host_group(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let group = super::host_groups::create(
            connection,
            CreateHostGroup::new(
                HostGroupName::new(resolve_string(attributes, "name", refs)?)?,
                resolve_string(attributes, "description", refs)?,
                resolve_string_vec(attributes, "hosts", refs)?
                    .into_iter()
                    .map(Hostname::new)
                    .collect::<Result<Vec<_>, _>>()?,
                resolve_string_vec(attributes, "parent_groups", refs)?
                    .into_iter()
                    .map(HostGroupName::new)
                    .collect::<Result<Vec<_>, _>>()?,
                resolve_string_vec(attributes, "owner_groups", refs)?
                    .into_iter()
                    .map(OwnerGroupName::new)
                    .collect::<Result<Vec<_>, _>>()?,
            )?,
        )?;
        Ok(Value::String(group.name().as_str().to_string()))
    }

    fn import_bacnet_id(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let assignment = super::bacnet_ids::create(
            connection,
            CreateBacnetIdAssignment::new(
                BacnetIdentifier::new(resolve_u32(attributes, "bacnet_id")?.ok_or_else(|| {
                    AppError::validation("missing required import attribute 'bacnet_id'")
                })?)?,
                Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
            ),
        )?;
        Ok(Value::String(assignment.bacnet_id().as_u32().to_string()))
    }

    fn import_ptr_override(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let ptr = super::ptr_overrides::create(
            connection,
            CreatePtrOverride::new(
                Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
                IpAddressValue::new(resolve_string(attributes, "address", refs)?)?,
                resolve_optional_string(attributes, "target_name", refs)?
                    .map(DnsName::new)
                    .transpose()?,
            ),
        )?;
        Ok(Value::String(ptr.address().as_str()))
    }

    fn import_network_policy(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let policy = super::network_policies::create(
            connection,
            CreateNetworkPolicy::new(
                NetworkPolicyName::new(resolve_string(attributes, "name", refs)?)?,
                resolve_string(attributes, "description", refs)?,
                resolve_optional_string(attributes, "community_template_pattern", refs)?,
            )?,
        )?;
        Ok(Value::String(policy.name().as_str().to_string()))
    }

    fn import_network_policy_attribute(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let name = resolve_string(attributes, "name", refs)?;
        let description = resolve_string(attributes, "description", refs)?;
        sql_query(
            "INSERT INTO network_policy_attributes (name, description)
             VALUES ($1, $2)",
        )
        .bind::<Text, _>(&name)
        .bind::<Text, _>(&description)
        .execute(connection)
        .map_err(map_unique("network policy attribute already exists"))?;
        Ok(Value::String(name))
    }

    fn import_network_policy_attribute_value(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let policy_name = resolve_string(attributes, "policy_name", refs)?;
        let attribute_name = resolve_string(attributes, "attribute_name", refs)?;
        let value = resolve_bool(attributes, "value")?
            .ok_or_else(|| AppError::validation("missing required import attribute 'value'"))?;
        let updated = sql_query(
            "INSERT INTO network_policy_attribute_values (policy_id, attribute_id, value)
             SELECT p.id, a.id, $3
             FROM network_policies p, network_policy_attributes a
             WHERE p.name = $1 AND a.name = $2",
        )
        .bind::<Text, _>(&policy_name)
        .bind::<Text, _>(&attribute_name)
        .bind::<Bool, _>(value)
        .execute(connection)
        .map_err(map_unique("network policy attribute value already exists"))?;
        if updated == 0 {
            return Err(AppError::not_found(format!(
                "policy '{}' or attribute '{}' was not found",
                policy_name, attribute_name
            )));
        }
        Ok(Value::String(format!("{policy_name}:{attribute_name}")))
    }

    fn import_community(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let community = super::communities::create(
            connection,
            CreateCommunity::new(
                NetworkPolicyName::new(resolve_string(attributes, "policy_name", refs)?)?,
                CidrValue::new(resolve_required_one_of_string(
                    attributes,
                    &["network_cidr", "network"],
                    refs,
                )?)?,
                CommunityName::new(resolve_string(attributes, "name", refs)?)?,
                resolve_string(attributes, "description", refs)?,
            )?,
        )?;
        Ok(Value::String(community.name().as_str().to_string()))
    }

    fn import_forward_zone(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let name = ZoneName::new(resolve_string(attributes, "name", refs)?)?;
        let primary_ns = DnsName::new(resolve_string(attributes, "primary_ns", refs)?)?;
        let nameservers = resolve_string_vec(attributes, "nameservers", refs)?
            .into_iter()
            .map(DnsName::new)
            .collect::<Result<Vec<_>, _>>()?;
        let command = CreateForwardZone::new(
            name,
            primary_ns,
            nameservers.clone(),
            EmailAddressValue::new(resolve_string(attributes, "email", refs)?)?,
            SerialNumber::new(resolve_u64(attributes, "serial_no")?.unwrap_or(1))?,
            SoaSeconds::new(resolve_u32(attributes, "refresh")?.unwrap_or(10_800))?,
            SoaSeconds::new(resolve_u32(attributes, "retry")?.unwrap_or(3_600))?,
            SoaSeconds::new(resolve_u32(attributes, "expire")?.unwrap_or(1_814_400))?,
            Ttl::new(resolve_u32(attributes, "soa_ttl")?.unwrap_or(43_200))?,
            Ttl::new(resolve_u32(attributes, "default_ttl")?.unwrap_or(43_200))?,
        );
        let nameserver_ids = Self::lookup_nameserver_ids(connection, command.nameservers())?;
        use crate::db::schema::{forward_zone_nameservers, forward_zones};
        let row = diesel::insert_into(forward_zones::table)
            .values((
                forward_zones::name.eq(command.name().as_str()),
                forward_zones::primary_ns.eq(command.primary_ns().as_str()),
                forward_zones::email.eq(command.email().as_str()),
                forward_zones::serial_no.eq(command.serial_no().as_i64()),
                forward_zones::refresh.eq(command.refresh().as_i32()),
                forward_zones::retry.eq(command.retry().as_i32()),
                forward_zones::expire.eq(command.expire().as_i32()),
                forward_zones::soa_ttl.eq(command.soa_ttl().as_i32()),
                forward_zones::default_ttl.eq(command.default_ttl().as_i32()),
            ))
            .returning(ForwardZoneRow::as_returning())
            .get_result(connection)
            .map_err(map_unique("forward zone already exists"))?;
        for nameserver_id in nameserver_ids {
            diesel::insert_into(forward_zone_nameservers::table)
                .values((
                    forward_zone_nameservers::zone_id.eq(row.id()),
                    forward_zone_nameservers::nameserver_id.eq(nameserver_id),
                ))
                .execute(connection)?;
        }
        let zone = row.into_domain(command.nameservers().to_vec())?;
        Ok(Value::String(zone.name().as_str().to_string()))
    }

    fn import_reverse_zone(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let name = ZoneName::new(resolve_string(attributes, "name", refs)?)?;
        let primary_ns = DnsName::new(resolve_string(attributes, "primary_ns", refs)?)?;
        let nameservers = resolve_string_vec(attributes, "nameservers", refs)?
            .into_iter()
            .map(DnsName::new)
            .collect::<Result<Vec<_>, _>>()?;
        let command = CreateReverseZone::new(
            name,
            resolve_optional_string(attributes, "network", refs)?
                .map(CidrValue::new)
                .transpose()?,
            primary_ns,
            nameservers.clone(),
            EmailAddressValue::new(resolve_string(attributes, "email", refs)?)?,
            SerialNumber::new(resolve_u64(attributes, "serial_no")?.unwrap_or(1))?,
            SoaSeconds::new(resolve_u32(attributes, "refresh")?.unwrap_or(10_800))?,
            SoaSeconds::new(resolve_u32(attributes, "retry")?.unwrap_or(3_600))?,
            SoaSeconds::new(resolve_u32(attributes, "expire")?.unwrap_or(1_814_400))?,
            Ttl::new(resolve_u32(attributes, "soa_ttl")?.unwrap_or(43_200))?,
            Ttl::new(resolve_u32(attributes, "default_ttl")?.unwrap_or(43_200))?,
        );
        let nameserver_ids = Self::lookup_nameserver_ids(connection, command.nameservers())?;
        let row = sql_query(
            "INSERT INTO reverse_zones
                (name, network, primary_ns, email, serial_no, refresh, retry, expire, soa_ttl, default_ttl)
             VALUES
                ($1, $2::cidr, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id, name::text AS name, network::text AS network, updated,
                       primary_ns::text AS primary_ns, email::text AS email, serial_no,
                       serial_no_updated_at, refresh, retry, expire, soa_ttl, default_ttl,
                       created_at, updated_at",
        )
        .bind::<Text, _>(command.name().as_str())
        .bind::<Nullable<Text>, _>(command.network().map(|value| value.as_str()))
        .bind::<Text, _>(command.primary_ns().as_str())
        .bind::<Text, _>(command.email().as_str())
        .bind::<diesel::sql_types::BigInt, _>(command.serial_no().as_i64())
        .bind::<Integer, _>(command.refresh().as_i32())
        .bind::<Integer, _>(command.retry().as_i32())
        .bind::<Integer, _>(command.expire().as_i32())
        .bind::<Integer, _>(command.soa_ttl().as_i32())
        .bind::<Integer, _>(command.default_ttl().as_i32())
        .get_result::<ReverseZoneRow>(connection)
        .map_err(map_unique("reverse zone already exists"))?;
        for nameserver_id in nameserver_ids {
            sql_query(
                "INSERT INTO reverse_zone_nameservers (zone_id, nameserver_id)
                 VALUES ($1, $2)",
            )
            .bind::<SqlUuid, _>(row.id())
            .bind::<SqlUuid, _>(nameserver_id)
            .execute(connection)?;
        }
        let zone = row.into_domain(command.nameservers().to_vec())?;
        Ok(Value::String(zone.name().as_str().to_string()))
    }

    fn import_forward_zone_delegation(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let delegation = Self::create_forward_zone_delegation_impl(
            connection,
            CreateForwardZoneDelegation::new(
                ZoneName::new(resolve_required_one_of_string(
                    attributes,
                    &["zone_name", "zone"],
                    refs,
                )?)?,
                DnsName::new(resolve_string(attributes, "name", refs)?)?,
                resolve_optional_string(attributes, "comment", refs)?.unwrap_or_default(),
                resolve_string_vec(attributes, "nameservers", refs)?
                    .into_iter()
                    .map(DnsName::new)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )?;
        Ok(Value::String(delegation.name().as_str().to_string()))
    }

    fn import_reverse_zone_delegation(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let delegation = Self::create_reverse_zone_delegation_impl(
            connection,
            CreateReverseZoneDelegation::new(
                ZoneName::new(resolve_required_one_of_string(
                    attributes,
                    &["zone_name", "zone"],
                    refs,
                )?)?,
                DnsName::new(resolve_string(attributes, "name", refs)?)?,
                resolve_optional_string(attributes, "comment", refs)?.unwrap_or_default(),
                resolve_string_vec(attributes, "nameservers", refs)?
                    .into_iter()
                    .map(DnsName::new)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )?;
        Ok(Value::String(delegation.name().as_str().to_string()))
    }

    fn import_excluded_range(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let network = CidrValue::new(resolve_string(attributes, "network", refs)?)?;
        let command = CreateExcludedRange::new(
            IpAddressValue::new(resolve_string(attributes, "start_ip", refs)?)?,
            IpAddressValue::new(resolve_string(attributes, "end_ip", refs)?)?,
            resolve_string(attributes, "description", refs)?,
        )?;
        let network_row = Self::query_network_by_cidr(connection, &network)?;
        if !network_row.contains(command.start_ip()) || !network_row.contains(command.end_ip()) {
            return Err(AppError::validation(
                "excluded range must be fully contained inside the network",
            ));
        }
        let overlap = sql_query(
            "SELECT id
             FROM network_excluded_ranges
             WHERE network_id = $1
               AND start_ip <= $3::inet
               AND end_ip >= $2::inet
             LIMIT 1",
        )
        .bind::<SqlUuid, _>(network_row.id())
        .bind::<Text, _>(command.start_ip().as_str())
        .bind::<Text, _>(command.end_ip().as_str())
        .get_result::<UuidRow>(connection)
        .optional()?;
        if overlap.is_some() {
            return Err(AppError::conflict(
                "excluded range overlaps an existing excluded range",
            ));
        }
        let range = sql_query(
            "INSERT INTO network_excluded_ranges (network_id, start_ip, end_ip, description)
             VALUES ($1, $2::inet, $3::inet, $4)
             RETURNING id, network_id, host(start_ip) AS start_ip, host(end_ip) AS end_ip,
                       description, created_at, updated_at",
        )
        .bind::<SqlUuid, _>(network_row.id())
        .bind::<Text, _>(command.start_ip().as_str())
        .bind::<Text, _>(command.end_ip().as_str())
        .bind::<Text, _>(command.description())
        .get_result::<ExcludedRangeRow>(connection)
        .map_err(map_unique("excluded range already exists"))?
        .into_domain()?;
        Ok(Value::String(format!(
            "{}:{}-{}",
            network.as_str(),
            range.start_ip().as_str(),
            range.end_ip().as_str()
        )))
    }

    fn import_host(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        use crate::db::schema::forward_zones;

        let command = CreateHost::new(
            Hostname::new(resolve_string(attributes, "name", refs)?)?,
            resolve_optional_string(attributes, "zone", refs)?
                .map(ZoneName::new)
                .transpose()?,
            resolve_u32(attributes, "ttl")?.map(Ttl::new).transpose()?,
            resolve_optional_string(attributes, "comment", refs)?.unwrap_or_default(),
        )?;
        let zone_name = command.zone().map(|zone| zone.as_str().to_string());
        let zone_id = match zone_name.as_ref() {
            Some(zone) => Some(
                forward_zones::table
                    .filter(forward_zones::name.eq(zone))
                    .select(forward_zones::id)
                    .first::<Uuid>(connection)
                    .optional()?
                    .ok_or_else(|| {
                        AppError::not_found(format!("forward zone '{}' was not found", zone))
                    })?,
            ),
            None => None,
        };
        let host = sql_query(
            "INSERT INTO hosts (name, zone_id, ttl, comment)
             VALUES ($1, $2, $3, $4)
             RETURNING id, name::text AS name, $5::text AS zone_name, ttl, comment, created_at, updated_at",
        )
        .bind::<Text, _>(command.name().as_str())
        .bind::<Nullable<SqlUuid>, _>(zone_id)
        .bind::<Nullable<Integer>, _>(command.ttl().map(|ttl| ttl.as_i32()))
        .bind::<Text, _>(command.comment())
        .bind::<Nullable<Text>, _>(zone_name)
        .get_result::<HostRow>(connection)
        .map_err(map_unique("host already exists"))?
        .into_domain()?;
        Ok(Value::String(host.name().as_str().to_string()))
    }

    fn import_host_attachment(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let attachment = Self::create_attachment_tx(
            connection,
            CreateHostAttachment::new(
                Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
                CidrValue::new(resolve_string(attributes, "network", refs)?)?,
                resolve_optional_string(attributes, "mac_address", refs)?
                    .map(MacAddressValue::new)
                    .transpose()?,
                resolve_optional_string(attributes, "comment", refs)?,
            ),
        )?;
        Ok(Value::String(attachment.id().to_string()))
    }

    fn import_ip_address(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let attachment = resolve_uuid(attributes, "attachment_id", refs)?
            .map(|attachment_id| Self::query_attachment_by_id(connection, attachment_id))
            .transpose()?;
        if let Some(attachment) = &attachment {
            if let Some(explicit_network) = resolve_optional_string(attributes, "network", refs)?
                && explicit_network != attachment.network_cidr().as_str()
            {
                return Err(AppError::validation(
                    "import ip_address network does not match referenced attachment",
                ));
            }
            if let Some(explicit_host) = resolve_optional_string(attributes, "host_name", refs)?
                && explicit_host != attachment.host_name().as_str()
            {
                return Err(AppError::validation(
                    "import ip_address host_name does not match referenced attachment",
                ));
            }
        }
        let host_name = attachment
            .as_ref()
            .map(|value| value.host_name().clone())
            .unwrap_or(Hostname::new(resolve_string(
                attributes,
                "host_name",
                refs,
            )?)?);
        let command = AssignIpAddress::new(
            host_name,
            resolve_optional_string(attributes, "address", refs)?
                .map(IpAddressValue::new)
                .transpose()?,
            match attachment.as_ref() {
                Some(value) => Some(value.network_cidr().clone()),
                None => resolve_optional_string(attributes, "network", refs)?
                    .map(CidrValue::new)
                    .transpose()?,
            },
            match attachment.as_ref() {
                Some(value) => value.mac_address().cloned(),
                None => resolve_optional_string(attributes, "mac_address", refs)?
                    .map(MacAddressValue::new)
                    .transpose()?,
            },
        )?;
        let assignment = Self::assign_ip_address_tx(connection, command)?;
        Ok(Value::String(assignment.address().as_str()))
    }

    fn import_attachment_dhcp_identifier(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let attachment_id = resolve_uuid(attributes, "attachment_id", refs)?.ok_or_else(|| {
            AppError::validation(
                "missing required import attribute 'attachment_id' or 'attachment_id_ref'",
            )
        })?;
        let family = match resolve_u64(attributes, "family")? {
            Some(4) => DhcpIdentifierFamily::V4,
            Some(6) => DhcpIdentifierFamily::V6,
            Some(_) => {
                return Err(AppError::validation(
                    "attachment DHCP identifier family must be 4 or 6",
                ));
            }
            None => {
                return Err(AppError::validation(
                    "missing required import attribute 'family'",
                ));
            }
        };
        let kind = match resolve_string(attributes, "kind", refs)?.as_str() {
            "client_id" => DhcpIdentifierKind::ClientId,
            "duid_llt" => DhcpIdentifierKind::DuidLlt,
            "duid_en" => DhcpIdentifierKind::DuidEn,
            "duid_ll" => DhcpIdentifierKind::DuidLl,
            "duid_uuid" => DhcpIdentifierKind::DuidUuid,
            "duid_raw" => DhcpIdentifierKind::DuidRaw,
            _ => {
                return Err(AppError::validation(
                    "unsupported attachment DHCP identifier kind",
                ));
            }
        };
        let identifier = Self::create_attachment_dhcp_identifier_tx(
            connection,
            CreateAttachmentDhcpIdentifier::new(
                attachment_id,
                family,
                kind,
                resolve_string(attributes, "value", refs)?,
                DhcpPriority::new(resolve_i32(attributes, "priority")?.unwrap_or(100)),
            )?,
        )?;
        Ok(Value::String(identifier.id().to_string()))
    }

    fn import_attachment_prefix_reservation(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let attachment_id = resolve_uuid(attributes, "attachment_id", refs)?.ok_or_else(|| {
            AppError::validation(
                "missing required import attribute 'attachment_id' or 'attachment_id_ref'",
            )
        })?;
        let reservation = Self::create_attachment_prefix_reservation_tx(
            connection,
            CreateAttachmentPrefixReservation::new(
                attachment_id,
                CidrValue::new(resolve_string(attributes, "prefix", refs)?)?,
            )?,
        )?;
        Ok(Value::String(reservation.id().to_string()))
    }

    fn import_attachment_community_assignment(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let attachment_id = resolve_uuid(attributes, "attachment_id", refs)?.ok_or_else(|| {
            AppError::validation(
                "missing required import attribute 'attachment_id' or 'attachment_id_ref'",
            )
        })?;
        let assignment = Self::create_attachment_community_assignment_tx(
            connection,
            CreateAttachmentCommunityAssignment::new(
                attachment_id,
                NetworkPolicyName::new(resolve_string(attributes, "policy_name", refs)?)?,
                CommunityName::new(resolve_string(attributes, "community_name", refs)?)?,
            ),
        )?;
        Ok(Value::String(assignment.id().to_string()))
    }

    fn import_record(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let owner_kind = resolve_optional_string(attributes, "owner_kind", refs)?
            .map(|raw| serde_json::from_value(Value::String(raw)).map_err(AppError::internal))
            .transpose()?;
        let command = CreateRecordInstance::with_reference(
            RecordTypeName::new(resolve_string(attributes, "type_name", refs)?)?,
            owner_kind,
            resolve_string(attributes, "owner_name", refs)?,
            resolve_optional_string(attributes, "anchor_name", refs)?,
            resolve_u32(attributes, "ttl")?.map(Ttl::new).transpose()?,
            attributes.get("data").cloned(),
            resolve_optional_string(attributes, "raw_rdata", refs)?
                .map(RawRdataValue::from_presentation)
                .transpose()?,
        )?;
        let record_type = Self::query_record_type_by_name(connection, command.type_name())?;
        let (anchor_id, _anchor_name, zone_id) = Self::resolve_record_owner(
            connection,
            command.owner_kind(),
            command.anchor_name(),
            command.owner_name(),
        )?;
        let validated = record_type.validate_record_input(
            command.owner_name(),
            command.data(),
            command.raw_rdata(),
        )?;
        let same_owner_records =
            Self::query_existing_owner_records(connection, command.owner_name())?;
        let existing_rrset = Self::query_rrset_by_type_and_owner(
            connection,
            record_type.id(),
            command.owner_name(),
        )?;
        let same_rrset_records = if let Some(rrset) = &existing_rrset {
            Self::query_existing_rrset_records(connection, rrset.id())?
        } else {
            Vec::new()
        };
        let alias_lookup = match &validated {
            ValidatedRecordContent::Structured(normalized) => Self::query_alias_owner_names(
                connection,
                &alias_target_names(normalized, record_type.name()),
            )?,
            ValidatedRecordContent::RawRdata(_) => BTreeMap::new(),
        };
        let alias_owner_names = alias_lookup
            .into_iter()
            .filter_map(|(name, is_alias)| is_alias.then_some(name))
            .collect();
        validate_record_relationships(
            &record_type,
            command.ttl(),
            &validated,
            &same_owner_records,
            &same_rrset_records,
            &alias_owner_names,
        )?;
        let rrset = if let Some(rrset) = existing_rrset {
            rrset
        } else {
            Self::insert_rrset(connection, &record_type, &command, anchor_id, zone_id)?
        };
        let rendered = if let ValidatedRecordContent::Structured(normalized) = &validated {
            Self::render_record_data(record_type.schema().render_template(), normalized)?
        } else {
            None
        };
        let record = Self::insert_record(connection, &rrset, rendered, &validated)?;
        Ok(Value::String(record.id().to_string()))
    }

    fn import_host_community_assignment(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let mapping = super::host_community_assignments::create(
            connection,
            CreateHostCommunityAssignment::new(
                Hostname::new(resolve_string(attributes, "host_name", refs)?)?,
                IpAddressValue::new(resolve_string(attributes, "address", refs)?)?,
                NetworkPolicyName::new(resolve_string(attributes, "policy_name", refs)?)?,
                CommunityName::new(resolve_string(attributes, "community_name", refs)?)?,
            ),
        )?;
        Ok(Value::String(mapping.id().to_string()))
    }

    fn import_host_policy_atom(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let command = CreateHostPolicyAtom::new(
            HostPolicyName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_string(attributes, "description", refs)?,
        );
        let row = sql_query(
            "INSERT INTO host_policy_atoms (name, description)
             VALUES ($1, $2)",
        )
        .bind::<Text, _>(command.name().as_str())
        .bind::<Text, _>(command.description())
        .execute(connection)
        .map_err(map_unique("host policy atom already exists"))?;
        debug_assert_eq!(row, 1);
        Ok(Value::String(command.name().as_str().to_string()))
    }

    fn import_host_policy_role(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let command = CreateHostPolicyRole::new(
            HostPolicyName::new(resolve_string(attributes, "name", refs)?)?,
            resolve_string(attributes, "description", refs)?,
        );
        let row = sql_query(
            "INSERT INTO host_policy_roles (name, description)
             VALUES ($1, $2)",
        )
        .bind::<Text, _>(command.name().as_str())
        .bind::<Text, _>(command.description())
        .execute(connection)
        .map_err(map_unique("host policy role already exists"))?;
        debug_assert_eq!(row, 1);
        Ok(Value::String(command.name().as_str().to_string()))
    }

    fn import_host_policy_role_atom(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let role_name = HostPolicyName::new(resolve_string(attributes, "role_name", refs)?)?;
        let atom_name = HostPolicyName::new(resolve_string(attributes, "atom_name", refs)?)?;
        let inserted = sql_query(
            "INSERT INTO host_policy_role_atoms (role_id, atom_id)
             SELECT r.id, a.id
             FROM host_policy_roles r, host_policy_atoms a
             WHERE r.name = $1 AND a.name = $2",
        )
        .bind::<Text, _>(role_name.as_str())
        .bind::<Text, _>(atom_name.as_str())
        .execute(connection)
        .map_err(map_unique("host policy role atom already exists"))?;
        if inserted == 0 {
            return Err(AppError::not_found(format!(
                "host policy role '{}' or atom '{}' was not found",
                role_name.as_str(),
                atom_name.as_str()
            )));
        }
        Ok(Value::String(format!(
            "{}:{}",
            role_name.as_str(),
            atom_name.as_str()
        )))
    }

    fn import_host_policy_role_host(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let role_name = HostPolicyName::new(resolve_string(attributes, "role_name", refs)?)?;
        let host_name = resolve_string(attributes, "host_name", refs)?;
        let inserted = sql_query(
            "INSERT INTO host_policy_role_hosts (role_id, host_id)
             SELECT r.id, h.id
             FROM host_policy_roles r, hosts h
             WHERE r.name = $1 AND h.name = $2",
        )
        .bind::<Text, _>(role_name.as_str())
        .bind::<Text, _>(&host_name)
        .execute(connection)
        .map_err(map_unique("host policy role host already exists"))?;
        if inserted == 0 {
            return Err(AppError::not_found(format!(
                "host policy role '{}' or host '{}' was not found",
                role_name.as_str(),
                host_name
            )));
        }
        Ok(Value::String(format!(
            "{}:{}",
            role_name.as_str(),
            host_name
        )))
    }

    fn import_host_policy_role_label(
        connection: &mut PgConnection,
        attributes: &Value,
        refs: &BTreeMap<String, String>,
    ) -> Result<Value, AppError> {
        let role_name = HostPolicyName::new(resolve_string(attributes, "role_name", refs)?)?;
        let label_name = resolve_string(attributes, "label_name", refs)?;
        let inserted = sql_query(
            "INSERT INTO host_policy_role_labels (role_id, label_id)
             SELECT r.id, l.id
             FROM host_policy_roles r, labels l
             WHERE r.name = $1 AND l.name = $2",
        )
        .bind::<Text, _>(role_name.as_str())
        .bind::<Text, _>(&label_name)
        .execute(connection)
        .map_err(map_unique("host policy role label already exists"))?;
        if inserted == 0 {
            return Err(AppError::not_found(format!(
                "host policy role '{}' or label '{}' was not found",
                role_name.as_str(),
                label_name
            )));
        }
        Ok(Value::String(format!(
            "{}:{}",
            role_name.as_str(),
            label_name
        )))
    }
}

#[async_trait]
impl ImportStore for PostgresStorage {
    async fn list_import_batches(
        &self,
        page: &PageRequest,
    ) -> Result<Page<ImportBatchSummary>, AppError> {
        let page = page.clone();
        self.database
            .run(move |c| {
                let items = Self::query_import_summaries(c)?;
                Ok(vec_to_page(items, &page))
            })
            .await
    }

    async fn create_import_batch(
        &self,
        command: CreateImportBatch,
    ) -> Result<ImportBatchSummary, AppError> {
        self.database
            .run(move |connection| {
                connection.transaction::<ImportBatchSummary, AppError, _>(|connection| {
                    use crate::db::schema::imports;

                    let import_id = Uuid::new_v4();
                    let task = Self::create_task_row(
                        connection,
                        &CreateTask::new(
                            "import_batch",
                            command.requested_by().map(str::to_string),
                            json!({"import_id": import_id}),
                            None,
                            1,
                        )?,
                        None,
                    )?;
                    diesel::insert_into(imports::table)
                        .values((
                            imports::id.eq(import_id),
                            imports::task_id.eq(Some(task.id())),
                            imports::status.eq("queued"),
                            imports::requested_by.eq(command.requested_by()),
                            imports::batch.eq(serde_json::to_value(command.batch())
                                .map_err(AppError::internal)?),
                        ))
                        .returning(ImportBatchRow::as_returning())
                        .get_result(connection)?
                        .into_summary()
                })
            })
            .await
    }

    async fn run_import_batch(&self, import_id: Uuid) -> Result<ImportBatchSummary, AppError> {
        let result = self
            .database
            .run(move |connection| {
                connection.transaction::<ImportBatchSummary, AppError, _>(|connection| {
                    let row = sql_query(
                        "SELECT id, task_id, status, requested_by, batch,
                                validation_report, commit_summary, created_at, updated_at
                         FROM imports
                         WHERE id = $1
                         FOR UPDATE",
                    )
                    .bind::<SqlUuid, _>(import_id)
                    .get_result::<ImportBatchRow>(connection)
                    .map_err(|_| {
                        AppError::not_found(format!("import batch '{}' was not found", import_id))
                    })?;
                    let batch = row.into_batch()?;
                    sql_query(
                        "UPDATE imports
                         SET status = 'validating', validation_report = $2, updated_at = now()
                         WHERE id = $1",
                    )
                    .bind::<SqlUuid, _>(import_id)
                    .bind::<diesel::sql_types::Jsonb, _>(json!({"valid": true}))
                    .execute(connection)?;

                    let mut refs = BTreeMap::new();
                    let mut applied = Vec::new();
                    for item in batch.items() {
                        let applied_item = Self::create_import_item(connection, item, &mut refs)
                            .map_err(|error| match error {
                                AppError::Config(message) => AppError::config(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Validation(message) => AppError::validation(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::NotFound(message) => AppError::not_found(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Conflict(message) => AppError::conflict(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Forbidden(message) => AppError::forbidden(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Unauthorized(message) => AppError::unauthorized(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Authz(message) => AppError::authz(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Unavailable(message) => AppError::unavailable(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                                AppError::Internal(message) => AppError::internal(format!(
                                    "import item '{}' ({}) failed: {}",
                                    item.reference(),
                                    item.kind(),
                                    message
                                )),
                            })?;
                        applied.push(applied_item);
                    }
                    let commit_summary = json!({
                        "applied": applied,
                        "count": batch.items().len(),
                    });
                    sql_query(
                        "UPDATE imports
                         SET status = 'succeeded', validation_report = $2,
                             commit_summary = $3, updated_at = now()
                         WHERE id = $1
                         RETURNING id, task_id, status, requested_by, batch,
                                   validation_report, commit_summary, created_at, updated_at",
                    )
                    .bind::<SqlUuid, _>(import_id)
                    .bind::<diesel::sql_types::Jsonb, _>(json!({"valid": true}))
                    .bind::<diesel::sql_types::Jsonb, _>(commit_summary)
                    .get_result::<ImportBatchRow>(connection)?
                    .into_summary()
                })
            })
            .await;

        match result {
            Ok(summary) => Ok(summary),
            Err(error) => {
                let message = error.to_string();
                let mark_failed: Result<(), AppError> = self
                    .database
                    .run(move |connection| {
                        sql_query(
                            "UPDATE imports
                             SET status = 'failed', validation_report = COALESCE(validation_report, $2), updated_at = now()
                             WHERE id = $1",
                        )
                        .bind::<SqlUuid, _>(import_id)
                        .bind::<diesel::sql_types::Jsonb, _>(json!({"error": message}))
                        .execute(connection)?;
                        Ok(())
                    })
                    .await;
                if let Err(mark_err) = mark_failed {
                    tracing::warn!(import_id = %import_id, error = %mark_err, "failed to mark import as failed");
                }
                Err(error)
            }
        }
    }
}
