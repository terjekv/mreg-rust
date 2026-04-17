# Pagination, Sorting, and Filtering

All list endpoints support cursor-based pagination, configurable sorting, and operator-based filtering. These features compose — you can filter, sort, and paginate in a single request.

## Filtering

### Query syntax

Filter parameters use the `field__operator=value` pattern:

```
GET /hosts?name__contains=prod&zone__iequals=example.org&comment__not_contains=test
```

- `field=value` is shorthand for `field__equals=value` (backwards compatible)
- `field__operator=value` applies an explicit operator
- Multiple conditions combine with AND
- The same field can have multiple conditions: `name__contains=foo&name__not_contains=bar`

### Operators

#### String fields

| Operator | Description | Example |
|----------|-------------|---------|
| `equals` | Exact match (default) | `?name=foo` or `?name__equals=foo` |
| `iequals` | Case-insensitive exact match | `?name__iequals=FOO` |
| `contains` | Substring match | `?name__contains=prod` |
| `icontains` | Case-insensitive substring | `?name__icontains=PROD` |
| `startswith` | Prefix match | `?name__startswith=api` |
| `istartswith` | Case-insensitive prefix | `?name__istartswith=API` |
| `endswith` | Suffix match | `?name__endswith=.org` |
| `iendswith` | Case-insensitive suffix | `?name__iendswith=.ORG` |
| `in` | Value in set (comma-separated) | `?name__in=host1,host2,host3` |
| `is_null` | Field is null/empty | `?zone__is_null=true` |

#### Numeric and date fields

| Operator | Description | Example |
|----------|-------------|---------|
| `equals` | Exact match | `?created_at__equals=2024-01-01T00:00:00Z` |
| `gt` | Greater than | `?created_at__gt=2024-01-01T00:00:00Z` |
| `gte` | Greater than or equal | `?created_at__gte=2024-01-01T00:00:00Z` |
| `lt` | Less than | `?created_at__lt=2025-01-01T00:00:00Z` |
| `lte` | Less than or equal | `?created_at__lte=2025-01-01T00:00:00Z` |
| `in` | Value in set | `?bacnet_id__in=100,200,300` |
| `is_null` | Field is null | `?ttl__is_null=true` |

### Negation

Any operator can be prefixed with `not_` to negate it:

- `?name__not_equals=bad` — exclude exact match
- `?name__not_contains=test` — exclude substring
- `?zone__not_is_null=true` — zone must be set (not null)
- `?created_at__not_gt=2025-01-01T00:00:00Z` — not after this date

### Operator validation

Each field has a type that determines which operators are valid. Using an invalid operator returns 400:

| Field type | Valid operators |
|-----------|----------------|
| **string** | All operators |
| **datetime** | equals, gt, gte, lt, lte, is_null (+ negations) |
| **numeric** | equals, gt, gte, lt, lte, in, is_null (+ negations) |
| **enum** | equals, in, is_null (+ negations) |
| **cidr** | equals, contains, startswith, in, is_null (+ negations) |

### Special filter fields

Only two fields have special semantics that don't use the operator pattern:

| Field | Endpoints | Behavior |
|-------|-----------|----------|
| `search` | hosts, networks, contacts, groups, policies, communities | Case-insensitive multi-column substring search |
| `contains_ip` | networks | Networks containing this IP address (Postgres CIDR operator) |

### Filterable fields by entity

#### Hosts — `GET /hosts`

| Field | Type | Operators |
|-------|------|-----------|
| `name` | string | All string operators |
| `zone` | string | All string operators + is_null |
| `comment` | string | All string operators |
| `created_at` | datetime | equals, gt, gte, lt, lte |
| `updated_at` | datetime | equals, gt, gte, lt, lte |
| `address` | string | All string operators (matches against assigned IPs) |
| `search` | special | Multi-column substring |

#### Networks — `GET /networks`

| Field | Type | Operators |
|-------|------|-----------|
| `description` | string | All string operators |
| `created_at` | datetime | equals, gt, gte, lt, lte |
| `updated_at` | datetime | equals, gt, gte, lt, lte |
| `family` | enum | equals, in (values: `4` or `6`) |
| `search` | special | CIDR + description substring |
| `contains_ip` | special | IP containment |

#### Records — `GET /records`

| Field | Type | Operators |
|-------|------|-----------|
| `type_name` | string | All string operators |
| `owner_kind` | enum | equals, in |
| `owner_name` | string | All string operators |

#### Host Contacts — `GET /host-contacts`

| Field | Type | Operators |
|-------|------|-----------|
| `email` | string | All string operators |
| `display_name` | string | All string operators |
| `created_at` | datetime | equals, gt, gte, lt, lte |
| `updated_at` | datetime | equals, gt, gte, lt, lte |
| `host` | string | All string operators (searches hosts list) |
| `search` | special | Email + display name substring |

#### Host Groups — `GET /host-groups`

| Field | Type | Operators |
|-------|------|-----------|
| `name` | string | All string operators |
| `description` | string | All string operators |
| `created_at` | datetime | equals, gt, gte, lt, lte |
| `updated_at` | datetime | equals, gt, gte, lt, lte |
| `host` | string | All string operators (searches hosts list) |
| `search` | special | Name + description substring |

#### BACnet IDs — `GET /bacnet-ids`

| Field | Type | Operators |
|-------|------|-----------|
| `bacnet_id` | numeric | equals, gt, gte, lt, lte, in |
| `host` | string | All string operators |

#### PTR Overrides — `GET /ptr-overrides`

| Field | Type | Operators |
|-------|------|-----------|
| `host` | string | All string operators |
| `address` | string | All string operators |

#### Network Policies — `GET /network-policies`

| Field | Type | Operators |
|-------|------|-----------|
| `name` | string | All string operators |
| `description` | string | All string operators |
| `created_at` | datetime | equals, gt, gte, lt, lte |
| `updated_at` | datetime | equals, gt, gte, lt, lte |
| `search` | special | Name + description substring |

#### Communities — `GET /communities`

| Field | Type | Operators |
|-------|------|-----------|
| `policy_name` | string | All string operators |
| `name` | string | All string operators |
| `description` | string | All string operators |
| `network` | cidr | equals, contains, startswith, in |
| `search` | special | Name + description substring |

#### Host Community Assignments — `GET /policy/network/host-community-assignments`

| Field | Type | Operators |
|-------|------|-----------|
| `community_name` | string | All string operators |
| `policy_name` | string | All string operators |
| `host` | string | All string operators |
| `address` | string | All string operators |

### Storage layer

- **Memory backend**: Filters are applied in Rust via `filter.matches()` methods
- **PostgreSQL backend**: Operator-based filters generate parameterized SQL WHERE clauses via `filter.sql_conditions()` and are pushed to the database. Special fields (`search`, `contains_ip`) are applied in SQL where possible, with fallback to Rust for cross-table logic
- Invalid operator/field combinations are rejected with 400 at the API layer before reaching storage

## Pagination

Pagination uses an opaque cursor (UUID) rather than offset/limit.

### Query parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `limit` | u64 | 100 | Maximum items per page (max 1000) |
| `after` | UUID | — | Cursor from previous page's `next_cursor` |

### Response shape

```json
{
  "items": [ ... ],
  "total": 42,
  "next_cursor": "550e8400-e29b-41d4-a716-446655440000"
}
```

- `total` reflects the count matching current filters (not just this page)
- `next_cursor` is `null` on the last page

## Sorting

### Query parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `sort_by` | string | `name` | Field to sort by (entity-specific) |
| `sort_dir` | string | `asc` | Sort direction: `asc` or `desc` |

### Sortable fields

| Entity | Default | Fields |
|--------|---------|--------|
| Host | name | name, comment, created_at, updated_at |
| Label | name | name, description, created_at, updated_at |
| Nameserver | name | name, created_at, updated_at |
| Zone | name | name, created_at, updated_at |
| Network | name | name, description, created_at, updated_at |
| Network | name | name (CIDR), description, created_at |

## Combining all three

```
GET /hosts?name__contains=prod&zone__iequals=example.org&sort_by=name&sort_dir=desc&limit=20&after=<cursor>
```

This returns up to 20 hosts matching both filter conditions, sorted by name descending, starting after the cursor.
