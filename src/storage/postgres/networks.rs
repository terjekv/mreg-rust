use async_trait::async_trait;
use diesel::{
    Connection, OptionalExtension, PgConnection, RunQueryDsl, sql_query,
    sql_types::{BigInt, Integer, Nullable, Text, Uuid as SqlUuid},
};

use crate::{
    db::models::{ExcludedRangeRow, IpAddressAssignmentRow, NetworkRow, UuidRow},
    domain::{
        filters::NetworkFilter,
        host::IpAddressAssignment,
        network::{
            CreateExcludedRange, CreateNetwork, ExcludedRange, Network, UpdateNetwork,
            cidr_contains, ip_to_u128, network_usable_bounds,
        },
        pagination::{Page, PageRequest, SortDirection, decode_cursor},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
    storage::NetworkStore,
};

use super::PostgresStorage;
use super::helpers::{
    TextValueRow, limited_rows_to_page_by, map_unique, run_count_query, run_dynamic_query,
    vec_to_page_by,
};

#[derive(diesel::QueryableByName)]
struct BigIntValueRow {
    #[diesel(sql_type = BigInt)]
    value: i64,
}

impl PostgresStorage {
    pub(super) fn query_networks(connection: &mut PgConnection) -> Result<Vec<Network>, AppError> {
        let rows = sql_query(
            "SELECT id, network::text AS network, description, vlan, dns_delegated, category, location, frozen, reserved, created_at, updated_at
             FROM networks
             ORDER BY network",
        )
        .load::<NetworkRow>(connection)?;
        rows.into_iter().map(NetworkRow::into_domain).collect()
    }

    pub(super) fn query_network_by_cidr(
        connection: &mut PgConnection,
        cidr: &CidrValue,
    ) -> Result<Network, AppError> {
        sql_query(
            "SELECT id, network::text AS network, description, vlan, dns_delegated, category, location, frozen, reserved, created_at, updated_at
             FROM networks
             WHERE network = $1::cidr",
        )
        .bind::<Text, _>(cidr.as_str())
        .get_result::<NetworkRow>(connection)
        .map_err(|_| AppError::not_found(format!("network '{}' was not found", cidr.as_str())))?
        .into_domain()
    }

    pub(super) fn query_network_by_id(
        connection: &mut PgConnection,
        network_id: uuid::Uuid,
    ) -> Result<Network, AppError> {
        sql_query(
            "SELECT id, network::text AS network, description, vlan, dns_delegated, category, location, frozen, reserved, created_at, updated_at
             FROM networks
             WHERE id = $1",
        )
        .bind::<SqlUuid, _>(network_id)
        .get_result::<NetworkRow>(connection)
        .map_err(|_| AppError::not_found("network was not found"))?
        .into_domain()
    }

    pub(super) fn query_network_containing_ip(
        connection: &mut PgConnection,
        address: &IpAddressValue,
    ) -> Result<Network, AppError> {
        sql_query(
            "SELECT id, network::text AS network, description, vlan, dns_delegated, category, location, frozen, reserved, created_at, updated_at
             FROM networks
             WHERE $1::inet <<= network
             ORDER BY masklen(network) DESC
             LIMIT 1",
        )
        .bind::<Text, _>(address.as_str())
        .get_result::<NetworkRow>(connection)
        .map_err(|_| {
            AppError::validation(format!(
                "IP address '{}' is not contained in any known network",
                address.as_str()
            ))
        })?
        .into_domain()
    }

    pub(super) fn query_excluded_ranges(
        connection: &mut PgConnection,
        network: &CidrValue,
    ) -> Result<Vec<ExcludedRange>, AppError> {
        let network_row = sql_query(
            "SELECT id, network::text AS network, description, vlan, dns_delegated, category, location, frozen, reserved, created_at, updated_at
             FROM networks
             WHERE network = $1::cidr",
        )
        .bind::<Text, _>(network.as_str())
        .get_result::<NetworkRow>(connection)
        .optional()?;

        let Some(network_row) = network_row else {
            return Ok(Vec::new());
        };

        let rows = sql_query(
            "SELECT id, network_id, host(start_ip) AS start_ip, host(end_ip) AS end_ip,
                    description, created_at, updated_at
             FROM network_excluded_ranges
             WHERE network_id = $1
             ORDER BY start_ip",
        )
        .bind::<SqlUuid, _>(network_row.into_domain()?.id())
        .load::<ExcludedRangeRow>(connection)?;

        rows.into_iter()
            .map(ExcludedRangeRow::into_domain)
            .collect()
    }

    pub(super) fn allocated_addresses_in_network(
        connection: &mut PgConnection,
        network: &Network,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        let rows = sql_query(
            "SELECT host(address) AS value
             FROM ip_addresses
             WHERE address <<= $1::cidr
             ORDER BY address",
        )
        .bind::<Text, _>(network.cidr().as_str())
        .load::<TextValueRow>(connection)?;

        rows.into_iter()
            .map(|row| IpAddressValue::new(row.value))
            .collect()
    }

    pub(super) fn count_allocated_addresses_in_network(
        connection: &mut PgConnection,
        network: &Network,
    ) -> Result<u64, AppError> {
        let row = sql_query(
            "SELECT COUNT(*)::bigint AS value
             FROM ip_addresses
             WHERE address <<= $1::cidr",
        )
        .bind::<Text, _>(network.cidr().as_str())
        .get_result::<BigIntValueRow>(connection)?;
        Ok(row.value.max(0) as u64)
    }

    pub(super) fn ensure_address_usable(
        connection: &mut PgConnection,
        network: &Network,
        address: &IpAddressValue,
    ) -> Result<(), AppError> {
        if network.frozen() {
            return Err(AppError::conflict("network is frozen"));
        }
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

        let overlap = sql_query(
            "SELECT id, network_id, host(start_ip) AS start_ip, host(end_ip) AS end_ip,
                    description, created_at, updated_at
             FROM network_excluded_ranges
             WHERE network_id = $1
               AND start_ip <= $2::inet
               AND end_ip >= $2::inet",
        )
        .bind::<SqlUuid, _>(network.id())
        .bind::<Text, _>(address.as_str())
        .get_result::<ExcludedRangeRow>(connection)
        .optional()?;
        if overlap.is_some() {
            return Err(AppError::validation(
                "IP address falls inside an excluded range",
            ));
        }

        let existing = sql_query("SELECT id FROM ip_addresses WHERE address = $1::inet")
            .bind::<Text, _>(address.as_str())
            .get_result::<UuidRow>(connection)
            .optional()?;
        if existing.is_some() {
            return Err(AppError::conflict(format!(
                "IP address '{}' is already allocated",
                address.as_str()
            )));
        }

        Ok(())
    }

    pub(super) fn allocate_address_in_network(
        connection: &mut PgConnection,
        network: &Network,
    ) -> Result<IpAddressValue, AppError> {
        // Lock the network row to serialize concurrent allocations
        sql_query("SELECT id FROM networks WHERE id = $1 FOR UPDATE")
            .bind::<SqlUuid, _>(network.id())
            .get_result::<UuidRow>(connection)
            .map_err(|_| AppError::not_found("network was not found"))?;

        let allocated: std::collections::HashSet<u128> =
            Self::allocated_addresses_in_network(connection, network)?
                .iter()
                .map(|a| ip_to_u128(a.as_inner()))
                .collect();
        let excluded = Self::query_excluded_ranges(connection, network.cidr())?;
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        let mut candidate = first;
        loop {
            if let Some(range) = excluded.iter().find(|range| {
                ip_to_u128(range.start_ip().as_inner()) <= candidate
                    && candidate <= ip_to_u128(range.end_ip().as_inner())
            }) {
                let end = ip_to_u128(range.end_ip().as_inner());
                if end >= last {
                    break;
                }
                candidate = end + 1;
                continue;
            }
            if !allocated.contains(&candidate) {
                return address_from_u128(network, candidate);
            }
            if candidate == last {
                break;
            }
            candidate += 1;
        }
        Err(AppError::conflict(
            "network has no remaining allocatable addresses",
        ))
    }

    pub(in crate::storage::postgres) fn list_networks_in_conn(
        connection: &mut PgConnection,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        let base = "SELECT n.id, n.network::text AS network, n.description, \
                n.vlan, n.dns_delegated, n.category, n.location, n.frozen, \
                n.reserved, n.created_at, n.updated_at \
                FROM networks n";

        let (clauses, mut values) = filter.sql_conditions();
        let where_str = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        let sort_by = page.sort_by().unwrap_or("network");
        let (order_col, cursor_cast) = match page.sort_by() {
            Some("description") => ("n.description", "text"),
            Some("created_at") => ("n.created_at", "timestamptz"),
            Some("updated_at") => ("n.updated_at", "timestamptz"),
            None => ("n.network::text", "text"),
            Some(other) => {
                return Err(AppError::validation(format!(
                    "unsupported sort_by field for networks: {other}"
                )));
            }
        };
        let order_dir = match page.sort_direction() {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        };
        let count_sql = format!("SELECT COUNT(*) AS count FROM ({base}{where_str}) AS _c");
        let total = run_count_query(connection, &count_sql, &values)?;

        let cursor_clause = if let Some(value) = page.after() {
            let cursor = decode_cursor(value, sort_by, page.sort_direction())?;
            let key_idx = values.len() + 1;
            values.push(cursor.key().to_string());
            let id_idx = values.len() + 1;
            values.push(cursor.id().to_string());
            let operator = match page.sort_direction() {
                SortDirection::Asc => ">",
                SortDirection::Desc => "<",
            };
            format!(
                " WHERE (_sort_key {operator} ${key_idx}::{cursor_cast}
                         OR (_sort_key = ${key_idx}::{cursor_cast} AND id > ${id_idx}::uuid))"
            )
        } else {
            String::new()
        };
        let limit_clause = if page.limit() != u64::MAX {
            format!(" LIMIT {}", page.limit() + 1)
        } else {
            String::new()
        };
        let query_str = format!(
            "WITH ranked AS (
                 SELECT n.id, n.network::text AS network, n.description, n.vlan,
                        n.dns_delegated, n.category, n.location, n.frozen, n.reserved,
                        n.created_at, n.updated_at,
                        {order_col} AS _sort_key
                 FROM networks n {where_str}
             )
             SELECT id, network, description, vlan, dns_delegated, category, location,
                    frozen, reserved, created_at, updated_at
             FROM ranked{cursor_clause} ORDER BY _sort_key {order_dir}, id{limit_clause}"
        );

        let rows = run_dynamic_query::<NetworkRow>(connection, &query_str, &values)?;
        let all_items: Vec<Network> = rows
            .into_iter()
            .map(NetworkRow::into_domain)
            .collect::<Result<Vec<_>, _>>()?;

        limited_rows_to_page_by(
            all_items,
            page,
            total,
            sort_by,
            page.sort_direction(),
            |network| match sort_by {
                "description" => network.description().to_string(),
                "created_at" => network.created_at().to_rfc3339(),
                "updated_at" => network.updated_at().to_rfc3339(),
                _ => network.cidr().as_str(),
            },
        )
    }

    pub(in crate::storage::postgres) fn create_network_in_conn(
        connection: &mut PgConnection,
        command: CreateNetwork,
    ) -> Result<Network, AppError> {
        let cidr = command.cidr().as_str();
        let description = command.description().to_string();
        let vlan = command.vlan().map(|v| v.as_i32());
        let dns_delegated = command.dns_delegated();
        let category = command.category().to_string();
        let location = command.location().to_string();
        let frozen = command.frozen();
        let reserved = command.reserved().as_i32();
        sql_query(
            "INSERT INTO networks (network, description, vlan, dns_delegated, category, location, frozen, reserved)
             VALUES ($1::cidr, $2, $3, $4, $5, $6, $7, $8)
             RETURNING id, network::text AS network, description, vlan, dns_delegated, category, location, frozen, reserved, created_at, updated_at",
        )
        .bind::<Text, _>(cidr)
        .bind::<Text, _>(description)
        .bind::<Nullable<Integer>, _>(vlan)
        .bind::<diesel::sql_types::Bool, _>(dns_delegated)
        .bind::<Text, _>(category)
        .bind::<Text, _>(location)
        .bind::<diesel::sql_types::Bool, _>(frozen)
        .bind::<Integer, _>(reserved)
        .get_result::<NetworkRow>(connection)
        .map_err(map_unique("network already exists"))?
        .into_domain()
    }

    pub(in crate::storage::postgres) fn update_network_in_conn(
        connection: &mut PgConnection,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        connection.transaction::<Network, AppError, _>(|connection| {
            let old = Self::query_network_by_cidr(connection, cidr)?;
            if old.frozen() && command.frozen != Some(false) {
                return Err(AppError::conflict(
                    "network is frozen; unfreeze it before changing it",
                ));
            }
            let reserved_value = command.reserved.unwrap_or(old.reserved());
            let (first, last) = network_usable_bounds(old.cidr(), reserved_value)?;
            if Self::allocated_addresses_in_network(connection, &old)?
                .iter()
                .any(|address| {
                    let value = ip_to_u128(address.as_inner());
                    value < first || value > last
                })
            {
                return Err(AppError::conflict(
                    "reserved space would include an allocated IP address",
                ));
            }
            let description = command
                .description
                .unwrap_or_else(|| old.description().to_string());
            let vlan: Option<i32> = command.vlan.resolve(old.vlan()).map(|v| v.as_i32());
            let dns_delegated = command.dns_delegated.unwrap_or(old.dns_delegated());
            let category = command
                .category
                .unwrap_or_else(|| old.category().to_string());
            let location = command
                .location
                .unwrap_or_else(|| old.location().to_string());
            let frozen = command.frozen.unwrap_or(old.frozen());
            let reserved: i32 = reserved_value.as_i32();

            sql_query(
                "UPDATE networks SET description = $1, vlan = $2, dns_delegated = $3, \
                 category = $4, location = $5, frozen = $6, reserved = $7, updated_at = now() \
                 WHERE network = $8::cidr \
                 RETURNING id, network::text AS network, description, vlan, dns_delegated, \
                 category, location, frozen, reserved, created_at, updated_at",
            )
            .bind::<Text, _>(description)
            .bind::<Nullable<Integer>, _>(vlan)
            .bind::<diesel::sql_types::Bool, _>(dns_delegated)
            .bind::<Text, _>(category)
            .bind::<Text, _>(location)
            .bind::<diesel::sql_types::Bool, _>(frozen)
            .bind::<Integer, _>(reserved)
            .bind::<Text, _>(cidr.as_str())
            .get_result::<NetworkRow>(connection)
            .map_err(|_| AppError::not_found(format!("network '{}' was not found", cidr.as_str())))?
            .into_domain()
        })
    }

    pub(in crate::storage::postgres) fn delete_network_in_conn(
        connection: &mut PgConnection,
        cidr: &CidrValue,
    ) -> Result<(), AppError> {
        let cidr_str = cidr.as_str();
        let network = Self::query_network_by_cidr(connection, cidr)?;
        if network.frozen() {
            return Err(AppError::conflict("network is frozen"));
        }
        let deleted = sql_query("DELETE FROM networks WHERE network = $1::cidr")
            .bind::<Text, _>(cidr_str.clone())
            .execute(connection)
            .map_err(|error| match error {
                diesel::result::Error::DatabaseError(
                    diesel::result::DatabaseErrorKind::ForeignKeyViolation,
                    _,
                ) => AppError::conflict("network is still referenced by other resources"),
                other => AppError::internal(other),
            })?;
        if deleted == 0 {
            return Err(AppError::not_found(format!(
                "network '{}' was not found",
                cidr_str
            )));
        }
        Ok(())
    }

    pub(in crate::storage::postgres) fn list_excluded_ranges_in_conn(
        connection: &mut PgConnection,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        let items = Self::query_excluded_ranges(connection, network)?;
        vec_to_page_by(items, page, "start_ip", &SortDirection::Asc, |item| {
            let family = if item.start_ip().as_inner().is_ipv4() {
                4
            } else {
                6
            };
            format!("{family}:{:039}", ip_to_u128(item.start_ip().as_inner()))
        })
    }

    pub(in crate::storage::postgres) fn add_excluded_range_in_conn(
        connection: &mut PgConnection,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        let start_ip = command.start_ip().as_str();
        let end_ip = command.end_ip().as_str();
        let description = command.description().to_string();
        connection.transaction::<ExcludedRange, AppError, _>(|connection| {
            let network_row = Self::query_network_by_cidr(connection, network)?;
            if network_row.frozen() {
                return Err(AppError::conflict("network is frozen"));
            }
            if !network_row.contains(command.start_ip()) || !network_row.contains(command.end_ip())
            {
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
            .bind::<Text, _>(start_ip.clone())
            .bind::<Text, _>(end_ip.clone())
            .get_result::<UuidRow>(connection)
            .optional()?;
            if overlap.is_some() {
                return Err(AppError::conflict(
                    "excluded range overlaps an existing excluded range",
                ));
            }
            let allocated = sql_query(
                "SELECT id FROM ip_addresses
                 WHERE address BETWEEN $1::inet AND $2::inet
                   AND address <<= $3::cidr
                 LIMIT 1",
            )
            .bind::<Text, _>(start_ip.clone())
            .bind::<Text, _>(end_ip.clone())
            .bind::<Text, _>(network_row.cidr().as_str())
            .get_result::<UuidRow>(connection)
            .optional()?;
            if allocated.is_some() {
                return Err(AppError::conflict(
                    "excluded range contains an allocated IP address",
                ));
            }
            sql_query(
                "INSERT INTO network_excluded_ranges (network_id, start_ip, end_ip, description)
                 VALUES ($1, $2::inet, $3::inet, $4)
                RETURNING id, network_id, host(start_ip) AS start_ip, host(end_ip) AS end_ip,
                          description, created_at, updated_at",
            )
            .bind::<SqlUuid, _>(network_row.id())
            .bind::<Text, _>(start_ip)
            .bind::<Text, _>(end_ip)
            .bind::<Text, _>(description)
            .get_result::<ExcludedRangeRow>(connection)
            .map_err(map_unique("excluded range already exists"))?
            .into_domain()
        })
    }

    pub(in crate::storage::postgres) fn list_used_addresses_in_conn(
        connection: &mut PgConnection,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        let rows = sql_query(
            "SELECT ia.id, ia.host_id, ia.attachment_id, host(ia.address) AS address, ia.family::int AS family, \
             nw.id AS network_id, ia.mac_address, ia.created_at, ia.updated_at \
             FROM ip_addresses ia \
             JOIN LATERAL ( \
               SELECT id FROM networks WHERE ia.address <<= network ORDER BY masklen(network) DESC LIMIT 1 \
             ) nw ON true \
             JOIN networks n ON ia.address <<= n.network \
             WHERE n.network = $1::cidr \
             ORDER BY ia.address",
        )
        .bind::<Text, _>(cidr.as_str())
        .load::<IpAddressAssignmentRow>(connection)?;

        rows.into_iter()
            .map(IpAddressAssignmentRow::into_domain)
            .collect()
    }

    pub(in crate::storage::postgres) fn list_unused_addresses_in_conn(
        connection: &mut PgConnection,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        let network = Self::query_network_by_cidr(connection, cidr)?;
        let limit = limit.unwrap_or(100) as usize;
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        let allocated = Self::allocated_addresses_in_network(connection, &network)?;
        let allocated_set: std::collections::HashSet<u128> =
            allocated.iter().map(|a| ip_to_u128(a.as_inner())).collect();
        let excluded = Self::query_excluded_ranges(connection, cidr)?;

        let mut result = Vec::new();
        match network.cidr().as_inner() {
            ipnet::IpNet::V4(_) => {
                for candidate in first..=last {
                    if result.len() >= limit {
                        break;
                    }
                    if allocated_set.contains(&candidate) {
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
                    if allocated_set.contains(&candidate) {
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

    pub(in crate::storage::postgres) fn count_unused_addresses_in_conn(
        connection: &mut PgConnection,
        cidr: &CidrValue,
    ) -> Result<u64, AppError> {
        let network = Self::query_network_by_cidr(connection, cidr)?;
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        let usable_span = last.saturating_sub(first).saturating_add(1);
        let allocated = Self::count_allocated_addresses_in_network(connection, &network)? as u128;
        let excluded = Self::query_excluded_ranges(connection, cidr)?;
        let excluded_count = excluded
            .iter()
            .map(|range| {
                let start = ip_to_u128(range.start_ip().as_inner()).max(first);
                let end = ip_to_u128(range.end_ip().as_inner()).min(last);
                if start > end { 0 } else { end - start + 1 }
            })
            .sum::<u128>();
        let unused = usable_span
            .saturating_sub(allocated)
            .saturating_sub(excluded_count);
        Ok(u64::try_from(unused).unwrap_or(u64::MAX))
    }

    /// Allocate a random usable address from the network.
    ///
    /// Locks the network row with SELECT FOR UPDATE to prevent concurrent
    /// allocations from picking the same address. Samples without materializing
    /// the address space, then falls back to the lowest free address when the
    /// network is dense.
    pub(super) fn allocate_random_address_in_network(
        connection: &mut PgConnection,
        network: &Network,
    ) -> Result<IpAddressValue, AppError> {
        use rand::Rng;

        // Lock the network row to serialize concurrent allocations
        sql_query("SELECT id FROM networks WHERE id = $1 FOR UPDATE")
            .bind::<SqlUuid, _>(network.id())
            .get_result::<UuidRow>(connection)
            .map_err(|_| AppError::not_found("network was not found"))?;

        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        let mut rng = rand::thread_rng();
        for _ in 0..256 {
            let chosen = if first == last {
                first
            } else {
                rng.gen_range(first..=last)
            };
            let address = address_from_u128(network, chosen)?;
            if Self::ensure_address_usable(connection, network, &address).is_ok() {
                return Ok(address);
            }
        }
        Self::allocate_address_in_network(connection, network)
    }
}

fn address_from_u128(network: &Network, value: u128) -> Result<IpAddressValue, AppError> {
    match network.cidr().as_inner() {
        ipnet::IpNet::V4(_) => {
            IpAddressValue::new(std::net::Ipv4Addr::from(value as u32).to_string())
        }
        ipnet::IpNet::V6(_) => IpAddressValue::new(std::net::Ipv6Addr::from(value).to_string()),
    }
}

#[async_trait]
impl NetworkStore for PostgresStorage {
    async fn list_networks(
        &self,
        page: &PageRequest,
        filter: &NetworkFilter,
    ) -> Result<Page<Network>, AppError> {
        let page = page.clone();
        let filter = filter.clone();
        self.database
            .run(move |c| Self::list_networks_in_conn(c, &page, &filter))
            .await
    }

    async fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError> {
        self.database
            .run(move |connection| Self::create_network_in_conn(connection, command))
            .await
    }

    async fn get_network_by_cidr(&self, cidr: &CidrValue) -> Result<Network, AppError> {
        let cidr = cidr.clone();
        self.database
            .run(move |connection| Self::query_network_by_cidr(connection, &cidr))
            .await
    }

    async fn update_network(
        &self,
        cidr: &CidrValue,
        command: UpdateNetwork,
    ) -> Result<Network, AppError> {
        let cidr = cidr.clone();
        self.database
            .run(move |connection| Self::update_network_in_conn(connection, &cidr, command))
            .await
    }

    async fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError> {
        let cidr = cidr.clone();
        self.database
            .run(move |connection| Self::delete_network_in_conn(connection, &cidr))
            .await
    }

    async fn list_excluded_ranges(
        &self,
        network: &CidrValue,
        page: &PageRequest,
    ) -> Result<Page<ExcludedRange>, AppError> {
        let network = network.clone();
        let page = page.clone();
        self.database
            .run(move |connection| {
                let items = Self::query_excluded_ranges(connection, &network)?;
                vec_to_page_by(items, &page, "start_ip", &SortDirection::Asc, |item| {
                    let family = if item.start_ip().as_inner().is_ipv4() {
                        4
                    } else {
                        6
                    };
                    format!("{family}:{:039}", ip_to_u128(item.start_ip().as_inner()))
                })
            })
            .await
    }

    async fn add_excluded_range(
        &self,
        network: &CidrValue,
        command: CreateExcludedRange,
    ) -> Result<ExcludedRange, AppError> {
        let network = network.clone();
        let start_ip = command.start_ip().as_str();
        let end_ip = command.end_ip().as_str();
        let description = command.description().to_string();
        self.database
            .run(move |connection| {
                connection.transaction::<ExcludedRange, AppError, _>(|connection| {
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
                    .bind::<Text, _>(start_ip.clone())
                    .bind::<Text, _>(end_ip.clone())
                    .get_result::<UuidRow>(connection)
                    .optional()?;
                    if overlap.is_some() {
                        return Err(AppError::conflict(
                            "excluded range overlaps an existing excluded range",
                        ));
                    }
                    sql_query(
                        "INSERT INTO network_excluded_ranges (network_id, start_ip, end_ip, description)
                         VALUES ($1, $2::inet, $3::inet, $4)
                        RETURNING id, network_id, host(start_ip) AS start_ip, host(end_ip) AS end_ip,
                                  description, created_at, updated_at",
                    )
                    .bind::<SqlUuid, _>(network_row.id())
                    .bind::<Text, _>(start_ip)
                    .bind::<Text, _>(end_ip)
                    .bind::<Text, _>(description)
                    .get_result::<ExcludedRangeRow>(connection)
                    .map_err(map_unique("excluded range already exists"))?
                    .into_domain()
                })
            })
            .await
    }

    async fn list_used_addresses(
        &self,
        cidr: &CidrValue,
    ) -> Result<Vec<IpAddressAssignment>, AppError> {
        let cidr = cidr.clone();
        self.database
            .run(move |connection| {
                let rows = sql_query(
                    "SELECT ia.id, ia.host_id, ia.attachment_id, host(ia.address) AS address, ia.family::int AS family, \
                     nw.id AS network_id, ia.mac_address, ia.created_at, ia.updated_at \
                     FROM ip_addresses ia \
                     JOIN LATERAL ( \
                       SELECT id FROM networks WHERE ia.address <<= network ORDER BY masklen(network) DESC LIMIT 1 \
                     ) nw ON true \
                     JOIN networks n ON ia.address <<= n.network \
                     WHERE n.network = $1::cidr \
                     ORDER BY ia.address",
                )
                .bind::<Text, _>(cidr.as_str())
                .load::<IpAddressAssignmentRow>(connection)?;

                rows.into_iter()
                    .map(IpAddressAssignmentRow::into_domain)
                    .collect()
            })
            .await
    }

    async fn list_unused_addresses(
        &self,
        cidr: &CidrValue,
        limit: Option<u32>,
    ) -> Result<Vec<IpAddressValue>, AppError> {
        let cidr = cidr.clone();
        self.database
            .run(move |connection| {
                let network = Self::query_network_by_cidr(connection, &cidr)?;
                let limit = limit.unwrap_or(100) as usize;
                let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
                let allocated = Self::allocated_addresses_in_network(connection, &network)?;
                let allocated_set: std::collections::HashSet<u128> =
                    allocated.iter().map(|a| ip_to_u128(a.as_inner())).collect();
                let excluded = Self::query_excluded_ranges(connection, &cidr)?;

                let mut result = Vec::new();
                match network.cidr().as_inner() {
                    ipnet::IpNet::V4(_) => {
                        for candidate in first..=last {
                            if result.len() >= limit {
                                break;
                            }
                            if allocated_set.contains(&candidate) {
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
                            if allocated_set.contains(&candidate) {
                                continue;
                            }
                            let addr = IpAddressValue::new(
                                std::net::Ipv6Addr::from(candidate).to_string(),
                            )?;
                            if excluded.iter().any(|r| r.contains(&addr)) {
                                continue;
                            }
                            result.push(addr);
                        }
                    }
                }
                Ok(result)
            })
            .await
    }

    async fn count_unused_addresses(&self, cidr: &CidrValue) -> Result<u64, AppError> {
        let cidr = cidr.clone();
        self.database
            .run(move |connection| {
                let network = Self::query_network_by_cidr(connection, &cidr)?;
                let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
                let usable_span = last.saturating_sub(first).saturating_add(1);
                let allocated =
                    Self::count_allocated_addresses_in_network(connection, &network)? as u128;
                let excluded = Self::query_excluded_ranges(connection, &cidr)?;
                let excluded_count = excluded
                    .iter()
                    .map(|range| {
                        let start = ip_to_u128(range.start_ip().as_inner()).max(first);
                        let end = ip_to_u128(range.end_ip().as_inner()).min(last);
                        if start > end { 0 } else { end - start + 1 }
                    })
                    .sum::<u128>();
                Ok(usable_span
                    .saturating_sub(allocated)
                    .saturating_sub(excluded_count) as u64)
            })
            .await
    }
}
