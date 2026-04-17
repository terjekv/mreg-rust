# How mreg-rust Differs from Django mreg

This document gives a high-level overview of what changed, what improved, and what was intentionally dropped or redesigned in the Rust reimplementation. For endpoint-by-endpoint details see [api-differences.md](api-differences.md); for migration steps see [migration-guide.md](migration-guide.md).

## Why rewrite?

The original [mreg](https://github.com/unioslo/mreg) is a Django REST framework application backed by PostgreSQL. It works, but over time several pain points emerged:

- **Performance at scale.** Django ORM queries and Python runtime overhead limit throughput for large zone exports and bulk operations.
- **Type safety.** Python's dynamic types let invalid data slip through to the database. DNS names, CIDRs, serial numbers, and TTLs deserve compile-time enforcement.
- **Tight coupling.** Django model serializers mix persistence, validation, and HTTP concerns in one layer. Adding a new record type means touching models, serializers, views, and URL configs.
- **Testing.** The Django test suite depends on a running PostgreSQL instance for every test. Fast, isolated in-memory testing is not practical.
- **Async work.** Background tasks (zone exports, bulk imports) would rely on Celery, adding operational complexity.

mreg-rust is not a line-by-line port. It preserves the domain concepts and API semantics but redesigns the internals from scratch.

## Architecture

| Aspect | Django mreg | mreg-rust |
| ------ | ---------- | --------- |
| Language | Python 3 | Rust (2024 edition) |
| Framework | Django REST framework | Actix-web 4 |
| ORM | Django ORM | Diesel 2 |
| Background tasks | None | Database-backed task queue |
| API docs | Semi-stable Swagger UI | Auto-generated OpenAPI 3 via utoipa, Swagger UI |
| Templating | Client-side via resource fetching | MiniJinja templates for zone files, DHCP configs, or user-created reports |
| Authentication | Django token auth | Configurable: none, local, LDAP, remote JWT scopes |
| Authorization | Modified Django permissions (internal) | Treetop policy engine (external) |
| Events | Django signals | Webhook, AMQP, Redis stream sinks (feature-flagged) |

### Layered design

mreg-rust separates concerns into five layers that Django mreg collapses into two (models + views):

```bash
API handlers    Parse HTTP, return DTOs. No business logic.
Services        Audit recording, event emission. Thin delegation.
Storage traits  Backend-neutral CRUD + query interfaces.
  memory/       HashMap backend for tests (no database needed).
  postgres/     Diesel backend for production.
Domain          Value objects, commands, validation. No I/O.
```

The storage layer is pluggable at startup (`MREG_STORAGE_BACKEND=auto|memory|postgres`). The in-memory backend enables the full test suite to run quickly without any external dependencies.

## Data model changes

### DNS records: one endpoint, not twelve

Django mreg has a separate model, serializer, view, and URL route for each record type (`/cnames/`, `/txts/`, `/mxs/`, `/srvs/`, ...). Adding a new type means touching all four layers.

mreg-rust has a single `POST /dns/records` endpoint that accepts a `type_name` field. 18 built-in types ship with RFC-aware validation schemas. Custom types can be registered at runtime and use RFC 3597 raw RDATA encoding.

### HINFO is a record, not a host field

Django mreg stores HINFO as a JSON field on the host model. mreg-rust treats it as a standard DNS record anchored to the host, just like MX or TXT.

### Wildcards are records, not hosts

Django mreg creates a host entity named `*.example.org`. mreg-rust represents wildcards as unanchored DNS records, which is closer to the DNS data model.

### UUIDs everywhere

All entity identifiers are UUIDs. Zone and host lookups still use natural keys (name, CIDR) in API paths. mreg-rust is consistent in using UUIDs for all internal references, never using mutable fields as identifiers for endpoints. This simplifies both the data model and the API design, avoiding issues with renaming and ensuring stable references across the system.

At scale, UUIDs will also allow for sharding and distributed storage if needed, without coupling the data model to a specific database schema or relying on auto-incrementing integers.

## API surface changes

### URL naming

Django mreg used concatenated or inconsistent paths. mreg-rust uses kebab-case throughout and organizes endpoints into logical groups:

| Group | Examples |
| ----- | -------- |
| `/dns/` | `forward-zones`, `reverse-zones`, `records`, `record-types`, `rrsets`, `ptr-overrides`, `nameservers` |
| `/inventory/` | `hosts`, `ip-addresses`, `networks`, `attachments`, `labels`, `host-contacts`, `host-groups`, `bacnet-ids` |
| `/policy/` | `host/atoms`, `host/roles`, `network/policies`, `network/communities` |
| `/workflows/` | `tasks`, `imports`, `export-templates`, `export-runs` |
| `/system/` | `health`, `version`, `status`, `history` |
| `/auth/` | `login`, `logout` |

### Pagination

Django mreg uses offset-based pagination (`?page=2&page_size=50`). mreg-rust uses cursor-based pagination with UUID cursors (`?limit=50&after=<cursor>`). Responses always include `{ items, total, next_cursor }`.

### Filtering and sorting

Django mreg uses DRF filter backends with `?name=foo&ordering=-name`. mreg-rust uses operator-based filtering (`?name__contains=prod&zone__iequals=example.org`) and explicit sort params (`?sort_by=name&sort_dir=desc`).

### Host creation with inline IP assignment

Both versions support creating a host with IPs in a single request. mreg-rust adds allocation policies:

```json
POST /inventory/hosts
{
  "name": "web.example.org",
  "zone": "example.org",
  "ip_addresses": [
    { "address": "10.0.1.50" },
    { "network": "10.0.2.0/24", "allocation": "first_free" },
    { "network": "fd00::/64", "allocation": "random", "mac_address": "aa:bb:cc:dd:ee:ff" }
  ]
}
```

The request is atomic: if any IP assignment fails, the host is not created.

## DHCP

Django mreg has limited DHCP support (MAC address on IP assignment, export scripts).

mreg-rust introduces a full DHCP data model:

- **Attachments** represent a host's network interface (NIC), with optional MAC address
- **DHCP identifiers** per attachment: IPv4 `client_id` or IPv6 DUID (LLT, EN, LL, UUID, raw), with priority ordering
- **Prefix reservations** for DHCPv6-PD
- **Auto-creation** of identifiers from MAC when IPs are assigned (configurable via `MREG_DHCP_AUTO_V4_CLIENT_ID` and `MREG_DHCP_AUTO_V6_DUID_LL`)
- **Built-in export templates** for Kea DHCPv4/v6, ISC DHCPd, and BIND zone files

See [dhcp-and-attachments.md](dhcp-and-attachments.md) for the full workflow.

## Export and import

### Exports

Django mreg uses custom Python scripts for zone file generation. mreg-rust uses MiniJinja templates with a well-defined context (zones, hosts, records, DHCP data). Nine built-in templates cover BIND zone files and Kea/ISC DHCP configs. Custom templates can be registered at runtime.

Exports run asynchronously via the task queue. Results are stored and retrievable.

### Imports

Django mreg imports data via per-endpoint API calls. mreg-rust adds a bulk import endpoint (`POST /workflows/imports`) that accepts a mixed-entity JSON batch with forward references, validates all items, and commits or rolls back atomically.

## Authentication

Django mreg uses Django REST framework token authentication.

mreg-rust supports two modes:

- **`none`** -- identity trusted from `X-Mreg-User`/`X-Mreg-Groups` headers (dev/test)
- **`scoped`** -- login with `scope:username` against configured backends (local users with Argon2id, LDAP bind, or remote JWT delegation), receiving an mreg-issued JWT

Authorization is still delegated to Treetop for policy evaluation, same as in Django mreg.

## Events and audit

Django mreg uses Django signals for side-effects. mreg-rust has:

- **Audit trail** -- immutable history events recorded in the service layer for every mutation, queryable via `GET /system/history`
- **Domain events** -- fire-and-forget delivery to webhook URLs, AMQP topic exchanges, or Redis streams (AMQP and Redis behind feature flags)

## What Django mreg has that mreg-rust does not

- **Network permissions** (`/permissions/netgroupregex/`) -- mreg-rust delegates all authorization to Treetop instead of maintaining its own permission tables
- **Force/override flags** -- the CLI's `-force` and `-override` flags have no equivalent; operations either succeed or fail based on constraints
- **Direct database import** from Django mreg's PostgreSQL schema -- planned but deferred (see [migration-backlog.md](migration-backlog.md))
- **Compatibility API layer** (`/api/compat/`) for legacy mreg-cli -- planned but not yet implemented (see [api-compatibility.md](api-compatibility.md))

## Testing

Django mreg requires a running PostgreSQL database for its test suite.

mreg-rust has a dual-backend test strategy:

- **Memory backend tests** run without any external dependencies (fast, CI-friendly)
- **PostgreSQL backend tests** run the same scenarios against a real database when `MREG_TEST_DATABASE_URL` is set
- The `dual_backend_test!` macro generates both variants from a single test function
- A shared `TestCtx` provides namespaced test data so tests run in parallel against one database without collisions

The test suite currently has 530+ tests covering API contracts, DNS record validation, filtering, pagination, sorting, host policy, DHCP, imports, exports, and cross-backend conformance.
