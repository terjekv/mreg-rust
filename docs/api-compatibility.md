# API Compatibility Layer

## Purpose

The `/api/compat/` prefix provides a translation layer that maps the old Django mreg API endpoints and payload formats to the mreg-rust internal API. This allows existing mreg-cli clients and integrations to work against the new server without modification.

The compatibility layer is a thin adapter — it deserializes old-format requests, transforms them to the internal domain model, calls the same storage layer, and serializes responses in the old format.

## Architecture

```
Old client → /api/compat/zones/forward/ → compat handler → ZoneStore → response → old format
New client → /api/v1/dns/forward-zones      → v1 handler     → ZoneStore → response → new format
```

Both paths use the same storage traits and domain types. The only difference is the HTTP surface: URL paths, request field names, and response shapes.

## Implementation location

```
src/api/compat/
├── mod.rs              configure() + shared helpers
├── zones.rs            /api/compat/zones/forward/, /api/compat/zones/reverse/
├── hosts.rs            /api/compat/hosts/
├── records.rs          /api/compat/cnames/, /api/compat/txts/, /api/compat/mxs/, etc.
├── networks.rs         /api/compat/networks/
├── ipaddresses.rs      /api/compat/ipaddresses/
├── hostgroups.rs       /api/compat/hostgroups/
├── hostpolicy.rs       /api/compat/hostpolicy/atoms/, /api/compat/hostpolicy/roles/
└── ptroverrides.rs     /api/compat/ptroverrides/
```

Registered in `src/api/mod.rs`:
```rust
cfg.service(web::scope("/api/compat").configure(compat::configure))
```

## Endpoint mapping

### Zones

| Old endpoint | Compat endpoint | Internal call |
|---|---|---|
| `GET /api/v1/zones/forward/` | `GET /api/compat/zones/forward/` | `ZoneStore::list_forward_zones` |
| `POST /api/v1/zones/forward/` | `POST /api/compat/zones/forward/` | `ZoneStore::create_forward_zone` |
| `GET /api/v1/zones/forward/{name}` | `GET /api/compat/zones/forward/{name}` | `ZoneStore::get_forward_zone_by_name` |
| `PATCH /api/v1/zones/forward/{name}` | `PATCH /api/compat/zones/forward/{name}` | `ZoneStore::update_forward_zone` |
| `DELETE /api/v1/zones/forward/{name}` | `DELETE /api/compat/zones/forward/{name}` | `ZoneStore::delete_forward_zone` |

Same pattern for reverse zones at `/api/compat/zones/reverse/`.

### Hosts

| Old endpoint | Compat endpoint | Internal call |
|---|---|---|
| `GET /api/v1/inventory/hosts/` | `GET /api/compat/hosts/` | `HostStore::list_hosts` |
| `POST /api/v1/inventory/hosts/` | `POST /api/compat/hosts/` | `HostStore::create_host` + optional `assign_ip_address` |
| `GET /api/v1/inventory/hosts/{name}` | `GET /api/compat/hosts/{name}` | `HostStore::get_host_by_name` |
| `PATCH /api/v1/inventory/hosts/{name}` | `PATCH /api/compat/hosts/{name}` | `HostStore::update_host` |
| `DELETE /api/v1/inventory/hosts/{name}` | `DELETE /api/compat/hosts/{name}` | `HostStore::delete_host` |

**Note**: The old `POST /hosts/` could create a host and assign an IP in one request. The compat handler should decompose this into two internal calls.

### Per-record-type endpoints

Each old record endpoint maps to the generic record store with a fixed `type_name`:

| Old endpoint | Compat endpoint | Maps to |
|---|---|---|
| `/api/v1/cnames/` | `/api/compat/cnames/` | `POST /records` with type_name=CNAME |
| `/api/v1/txts/` | `/api/compat/txts/` | `POST /records` with type_name=TXT |
| `/api/v1/mxs/` | `/api/compat/mxs/` | `POST /records` with type_name=MX |
| `/api/v1/srvs/` | `/api/compat/srvs/` | `POST /records` with type_name=SRV |
| `/api/v1/naptrs/` | `/api/compat/naptrs/` | `POST /records` with type_name=NAPTR |
| `/api/v1/sshfps/` | `/api/compat/sshfps/` | `POST /records` with type_name=SSHFP |
| `/api/v1/locs/` | `/api/compat/locs/` | `POST /records` with type_name=LOC |

The compat handler translates old field names to the generic record `data` payload. For example, old CNAME:
```json
// Old format:
{"host": 123, "name": "alias.example.org", "cname": "real.example.org", "ttl": 300}

// Translated to:
{"type_name": "CNAME", "owner_kind": "host", "owner_name": "alias.example.org",
 "ttl": 300, "data": {"target": "real.example.org"}}
```

### IP addresses

| Old endpoint | Compat endpoint | Internal call |
|---|---|---|
| `GET /api/v1/ipaddresses/` | `GET /api/compat/ipaddresses/` | `HostStore::list_ip_addresses` |
| `POST /api/v1/ipaddresses/` | `POST /api/compat/ipaddresses/` | `HostStore::assign_ip_address` |
| `PATCH /api/v1/ipaddresses/{id}` | `PATCH /api/compat/ipaddresses/{id}` | `HostStore::update_ip_address` |
| `DELETE /api/v1/ipaddresses/{id}` | `DELETE /api/compat/ipaddresses/{id}` | `HostStore::unassign_ip_address` |

**Note**: The old API uses integer IDs for IP addresses. The compat layer needs to map between old integer IDs and our UUID-based system.

### Networks

| Old endpoint | Compat endpoint | Internal call |
|---|---|---|
| `GET /api/v1/inventory/networks/` | `GET /api/compat/networks/` | `NetworkStore::list_networks` |
| `POST /api/v1/inventory/networks/` | `POST /api/compat/networks/` | `NetworkStore::create_network` |
| `GET /api/v1/inventory/networks/{network}` | `GET /api/compat/networks/{network}` | `NetworkStore::get_network_by_cidr` |
| `GET /api/v1/inventory/networks/{network}/used_addresses/` | `GET /api/compat/networks/{network}/used_addresses/` | `NetworkStore::list_used_addresses` |
| `GET /api/v1/inventory/networks/{network}/unused_addresses/` | `GET /api/compat/networks/{network}/unused_addresses/` | `NetworkStore::list_unused_addresses` |

### Host groups

| Old endpoint | Compat endpoint | Internal call |
|---|---|---|
| `GET /api/v1/hostgroups/` | `GET /api/compat/hostgroups/` | `AncillaryStore::list_host_groups` |
| `POST /api/v1/hostgroups/` | `POST /api/compat/hostgroups/` | `AncillaryStore::create_host_group` |

### Host policy

| Old endpoint | Compat endpoint | Internal call |
|---|---|---|
| `POST /api/v1/hostpolicy/atoms/` | `POST /api/compat/hostpolicy/atoms/` | `HostPolicyStore::create_atom` |
| `POST /api/v1/hostpolicy/roles/` | `POST /api/compat/hostpolicy/roles/` | `HostPolicyStore::create_role` |

## Payload translation

### Pagination

Old format (Django REST framework):
```json
{"count": 42, "next": "http://...", "previous": null, "results": [...]}
```

Compat translation from internal:
```json
// Internal: { items: [...], total: 42, next_cursor: "uuid" }
// Compat:   { count: 42, next: "/api/compat/hosts/?cursor=uuid", previous: null, results: [...] }
```

### Error responses

Old format:
```json
{"detail": "Not found."}
```

Compat translation from internal:
```json
// Internal: { error: "not_found", message: "host 'x' was not found" }
// Compat:   { detail: "host 'x' was not found" }
```

## Implementation status

The compatibility layer is planned but **not yet implemented**. It can be built incrementally, starting with the most-used endpoints (hosts, zones, networks) and expanding to cover the full old API surface.

## When to use which API

| Use case | Recommended API |
|----------|----------------|
| New integrations | `/api/v1/` — full feature set, modern design |
| Existing mreg-cli | `/api/compat/` — backwards compatible |
| Migration period | Both — `/api/compat/` for existing clients, `/api/v1/` for new work |
| Long term | `/api/v1/` only — compat layer may be sunset |
