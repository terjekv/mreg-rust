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
        pagination::{Page, PageRequest},
        types::{CidrValue, IpAddressValue},
    },
    errors::AppError,
    storage::NetworkStore,
};

use super::PostgresStorage;
use super::helpers::{
    TextValueRow, map_unique, rows_to_page, run_count_query, run_dynamic_query, vec_to_page,
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
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;
        for candidate in first..=last {
            if allocated.contains(&candidate) {
                continue;
            }
            let address = match network.cidr().as_inner() {
                ipnet::IpNet::V4(_) => {
                    IpAddressValue::new(std::net::Ipv4Addr::from(candidate as u32).to_string())?
                }
                ipnet::IpNet::V6(_) => {
                    IpAddressValue::new(std::net::Ipv6Addr::from(candidate).to_string())?
                }
            };
            if Self::ensure_address_usable(connection, network, &address).is_ok() {
                return Ok(address);
            }
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

        let (clauses, values) = filter.sql_conditions();
        let where_str = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };
        let order_col = match page.sort_by() {
            Some("description") => "n.description",
            Some("created_at") => "n.created_at",
            Some("updated_at") => "n.updated_at",
            None => "n.network::text",
            Some(other) => {
                return Err(AppError::validation(format!(
                    "unsupported sort_by field for networks: {other}"
                )));
            }
        };
        let order_dir = match page.sort_direction() {
            crate::domain::pagination::SortDirection::Asc => "ASC",
            crate::domain::pagination::SortDirection::Desc => "DESC",
        };
        let count_sql = format!("SELECT COUNT(*) AS count FROM ({base}{where_str}) AS _c");
        let total = run_count_query(connection, &count_sql, &values)?;

        let limit_clause = if page.after().is_none() && page.limit() != u64::MAX {
            format!(" LIMIT {}", page.limit() + 1)
        } else {
            String::new()
        };
        let query_str = format!(
            "{base}{where_str} ORDER BY {order_col} {order_dir}, n.id{limit_clause}"
        );

        let rows = run_dynamic_query::<NetworkRow>(connection, &query_str, &values)?;
        let all_items: Vec<Network> = rows
            .into_iter()
            .map(NetworkRow::into_domain)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows_to_page(all_items, page, total))
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
            let reserved: i32 = command.reserved.unwrap_or(old.reserved()).as_i32();

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
            .map_err(|_| {
                AppError::not_found(format!("network '{}' was not found", cidr.as_str()))
            })?
            .into_domain()
        })
    }

    pub(in crate::storage::postgres) fn delete_network_in_conn(
        connection: &mut PgConnection,
        cidr: &CidrValue,
    ) -> Result<(), AppError> {
        let cidr_str = cidr.as_str();
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
        Ok(vec_to_page(items, page))
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
            if !network_row.contains(command.start_ip())
                || !network_row.contains(command.end_ip())
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
        let allocated =
            Self::count_allocated_addresses_in_network(connection, &network)? as u128;
        let excluded = Self::query_excluded_ranges(connection, cidr)?;
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
    }

    /// Allocate a random usable address from the network.
    ///
    /// Locks the network row with SELECT FOR UPDATE to prevent concurrent
    /// allocations from picking the same address. Builds the full set of
    /// usable candidates and picks one at random.
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

        let allocated: std::collections::HashSet<u128> =
            Self::allocated_addresses_in_network(connection, network)?
                .iter()
                .map(|a| ip_to_u128(a.as_inner()))
                .collect();
        let excluded = Self::query_excluded_ranges(connection, network.cidr())?;
        let (first, last) = network_usable_bounds(network.cidr(), network.reserved())?;

        // Build the set of usable candidate values
        let candidates: Vec<u128> = (first..=last)
            .filter(|c| {
                if allocated.contains(c) {
                    return false;
                }
                let addr = match network.cidr().as_inner() {
                    ipnet::IpNet::V4(_) => {
                        IpAddressValue::new(std::net::Ipv4Addr::from(*c as u32).to_string())
                    }
                    ipnet::IpNet::V6(_) => {
                        IpAddressValue::new(std::net::Ipv6Addr::from(*c).to_string())
                    }
                };
                match addr {
                    Ok(a) => !excluded.iter().any(|r| r.contains(&a)),
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
            ipnet::IpNet::V6(_) => {
                IpAddressValue::new(std::net::Ipv6Addr::from(chosen).to_string())
            }
        }
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
            .run(move |c| {
                let base = "SELECT n.id, n.network::text AS network, n.description, \
                        n.vlan, n.dns_delegated, n.category, n.location, n.frozen, \
                        n.reserved, n.created_at, n.updated_at \
                        FROM networks n";

                let (clauses, values) = filter.sql_conditions();
                let where_str = if clauses.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", clauses.join(" AND "))
                };
                let order_col = match page.sort_by() {
                    Some("description") => "n.description",
                    Some("created_at") => "n.created_at",
                    Some("updated_at") => "n.updated_at",
                    None => "n.network::text",
                    Some(other) => {
                        return Err(AppError::validation(format!(
                            "unsupported sort_by field for networks: {other}"
                        )));
                    }
                };
                let order_dir = match page.sort_direction() {
                    crate::domain::pagination::SortDirection::Asc => "ASC",
                    crate::domain::pagination::SortDirection::Desc => "DESC",
                };
                let count_sql = format!("SELECT COUNT(*) AS count FROM ({base}{where_str}) AS _c");
                let total = run_count_query(c, &count_sql, &values)?;

                let limit_clause = if page.after().is_none() && page.limit() != u64::MAX {
                    format!(" LIMIT {}", page.limit() + 1)
                } else {
                    String::new()
                };
                let query_str = format!(
                    "{base}{where_str} ORDER BY {order_col} {order_dir}, n.id{limit_clause}"
                );

                let rows = run_dynamic_query::<NetworkRow>(c, &query_str, &values)?;
                let all_items: Vec<Network> = rows
                    .into_iter()
                    .map(NetworkRow::into_domain)
                    .collect::<Result<Vec<_>, _>>()?;

                // contains_ip is pushed to SQL via sql_conditions(); no Rust-side filter needed.
                Ok(rows_to_page(all_items, &page, total))
            })
            .await
    }

    async fn create_network(&self, command: CreateNetwork) -> Result<Network, AppError> {
        let cidr = command.cidr().as_str();
        let description = command.description().to_string();
        let vlan = command.vlan().map(|v| v.as_i32());
        let dns_delegated = command.dns_delegated();
        let category = command.category().to_string();
        let location = command.location().to_string();
        let frozen = command.frozen();
        let reserved = command.reserved().as_i32();
        self.database
            .run(move |connection| {
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
            })
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
            .run(move |connection| {
                connection.transaction::<Network, AppError, _>(|connection| {
                    let old = Self::query_network_by_cidr(connection, &cidr)?;
                    let description = command.description.unwrap_or_else(|| old.description().to_string());
                    let vlan: Option<i32> = command
                        .vlan
                        .resolve(old.vlan())
                        .map(|v| v.as_i32());
                    let dns_delegated = command.dns_delegated.unwrap_or(old.dns_delegated());
                    let category = command.category.unwrap_or_else(|| old.category().to_string());
                    let location = command.location.unwrap_or_else(|| old.location().to_string());
                    let frozen = command.frozen.unwrap_or(old.frozen());
                    let reserved: i32 = command.reserved.unwrap_or(old.reserved()).as_i32();

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
            })
            .await
    }

    async fn delete_network(&self, cidr: &CidrValue) -> Result<(), AppError> {
        let cidr = cidr.as_str().to_string();
        self.database
            .run(move |connection| {
                let deleted = sql_query("DELETE FROM networks WHERE network = $1::cidr")
                    .bind::<Text, _>(cidr.clone())
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
                        cidr
                    )));
                }
                Ok(())
            })
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
                Ok(vec_to_page(items, &page))
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
