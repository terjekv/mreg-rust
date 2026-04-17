# API Differences: mreg-rust vs Django mreg

This document describes the key differences between the mreg-rust REST API and the original Django-based mreg API for developers migrating clients or understanding the architectural changes.

## URL structure

### Path naming

| Concept | Old mreg | mreg-rust |
|---------|----------|-----------|
| Forward zones | `/api/v1/zones/forward/` | `/api/v1/dns/forward-zones` |
| Reverse zones | `/api/v1/zones/reverse/` | `/api/v1/dns/reverse-zones` |
| IP addresses | `/api/v1/ipaddresses/` | `/api/v1/inventory/ip-addresses` |
| PTR overrides | `/api/v1/ptroverrides/` | `/api/v1/dns/ptr-overrides` |
| Host groups | `/api/v1/hostgroups/` | `/api/v1/inventory/host-groups` |
| Host policy | `/api/v1/hostpolicy/atoms/` | `/api/v1/policy/host/atoms` |
| Host contacts | (on host model) | `/api/v1/inventory/host-contacts` |
| Communities | (nested under networks) | `/api/v1/policy/network/communities` |

mreg-rust uses kebab-case consistently. The old mreg used a mix of concatenated words and nested paths.

### Record type endpoints

**Old mreg** had separate endpoints per DNS record type:

```
/api/v1/cnames/
/api/v1/txts/
/api/v1/mxs/
/api/v1/srvs/
/api/v1/naptrs/
/api/v1/sshfps/
/api/v1/locs/
```

**mreg-rust** has a single generic record endpoint:

```
/api/v1/dns/records          (all record types)
/api/v1/dns/record-types     (type definitions)
/api/v1/dns/rrsets           (resource record sets)
```

To create a CNAME in mreg-rust:
```json
POST /api/v1/dns/records
{
  "type_name": "CNAME",
  "owner_kind": "host",
  "owner_name": "alias.example.org",
  "data": {"target": "real.example.org"}
}
```

This unified model supports all 18 built-in types plus runtime-defined custom types.

## Data model differences

### HINFO

**Old mreg**: HINFO was a field on the host model (`hinfo` field on `/api/v1/inventory/hosts/{name}`).

**mreg-rust**: HINFO is a regular DNS record type. Create it like any other record:
```json
POST /api/v1/dns/records
{
  "type_name": "HINFO",
  "owner_kind": "host",
  "owner_name": "server.example.org",
  "data": {"cpu": "x86_64", "os": "Linux"}
}
```

### Host contacts

**Old mreg**: Contact was a text field on the host model.

**mreg-rust**: Contacts are a separate entity (`/api/v1/inventory/host-contacts`) with email, display name, and a many-to-many relationship with hosts.

### Wildcard hosts

**Old mreg**: `*.example.org` was a regular host entity.

**mreg-rust**: Wildcards are unanchored DNS records, not hosts. See [wildcard-dns.md](wildcard-dns.md).

### IP address management

**Old mreg**: IPs could be managed inline when creating/updating hosts, or via `/api/v1/ipaddresses/`.

**mreg-rust**: IPs can be managed via `/api/v1/inventory/ip-addresses` or inline during host creation. `POST /api/v1/inventory/hosts` accepts an optional `ip_addresses` array:

```json
{
  "name": "web.example.org",
  "zone": "example.org",
  "comment": "Production web server",
  "ip_addresses": [
    { "address": "10.0.1.50" },
    { "network": "10.0.2.0/24", "allocation": "first_free" },
    { "network": "fd00::/64", "allocation": "random", "mac_address": "aa:bb:cc:dd:ee:ff" }
  ]
}
```

Each entry accepts `address` (explicit IP), `network` (CIDR for auto-allocation), `allocation` (`"first_free"` or `"random"`, defaults to `"first_free"`), and optional `mac_address`. The request is atomic — if any IP assignment fails, the host is not created. Omitting `ip_addresses` creates the host without IPs (standalone `POST /api/v1/inventory/ip-addresses` still works for later assignment). IP assignment auto-creates A/AAAA and PTR records.

### Network fields

**Old mreg**: Networks had frozen, vlan, category, location, dns_delegated fields from day one.

**mreg-rust**: Same fields are now available. `PATCH /networks/{cidr}` supports updating all of them.

## Pagination

**Old mreg**: Offset-based pagination (`?page=2&page_size=50`) with Django REST framework.

**mreg-rust**: Cursor-based pagination with UUID cursors:
```
GET /api/v1/inventory/hosts?limit=50
→ { items: [...], total: 123, next_cursor: "uuid-here" }

GET /api/v1/inventory/hosts?limit=50&after=uuid-here
→ next page
```

## Filtering

**Old mreg**: Django filter backends with `?name=foo&ordering=-created_at`.

**mreg-rust**: Operator-based filtering with `field__operator=value` syntax:
```
GET /api/v1/inventory/hosts?name__contains=prod&zone__iequals=example.org
GET /api/v1/inventory/hosts?created_at__gt=2024-01-01T00:00:00Z
GET /api/v1/inventory/networks?family=4&description__icontains=production
```

See [pagination-sort-filter.md](pagination-sort-filter.md) for the full operator reference.

## Sorting

**Old mreg**: `?ordering=name` or `?ordering=-name` (prefix `-` for descending).

**mreg-rust**: `?sort_by=name&sort_dir=desc` (explicit direction parameter).

## Authentication

**Old mreg**: Token-based auth via Django REST framework.

**mreg-rust**: Configurable authentication with two modes:

- `none`: trust `X-Mreg-User` and `X-Mreg-Groups` headers
- `scoped`: login as `scope:username` against configured `local`, `ldap`, or `remote` scopes, always returning an mreg-issued JWT for a namespace-aware principal

Authorization is still delegated to Treetop or the current development bypass/deny behavior. See [authentication.md](authentication.md) for the full flow.

## Response format

**Old mreg**: Django REST framework responses with nested pagination metadata.

**mreg-rust**: Consistent response shapes:
- List endpoints: `{ items: [...], total: N, next_cursor: "uuid"|null }`
- Create: 201 with created entity
- Update: 200 with updated entity
- Delete: 204 No Content
- Errors: `{ error: "type", message: "description" }`

## Features in mreg-rust not in old mreg

- **OpenAPI documentation** at `/swagger-ui/` with auto-generated specs
- **RFC 3597 raw RDATA** for custom/unknown DNS record types
- **Import/export workflows** for bulk operations
- **Audit trail** via `GET /api/v1/system/history`
- **18 built-in DNS record types** (including CAA, TLSA, SVCB, HTTPS)
- **Operator-based filtering** with negation, substring, comparison operators

## Features in old mreg not yet in mreg-rust

- **Network permissions** (`/api/v1/permissions/netgroupregex/`) — authorization is delegated to Treetop instead
- **DHCP management** — partially supported via MAC address on IP assignments
- **Force/override flags** — the old CLI's `-force` and `-override` flags are not replicated; operations either succeed or fail based on constraints
