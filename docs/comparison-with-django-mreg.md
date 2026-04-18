# How mreg-rust Differs from Django mreg

This document gives a high-level overview of what changed, what improved, and what was intentionally dropped or redesigned in the Rust reimplementation. For endpoint-by-endpoint details see [api-differences.md](api-differences.md); for migration steps see [migration-guide.md](migration-guide.md).

## Why rewrite?

The original [mreg](https://github.com/unioslo/mreg) is a Django REST framework application backed by PostgreSQL. It works, but over time several pain points emerged:

- **Mixing of domains.** MREG conceptually handles DNS (as per RFCs), inventory (hosts, IPs, networks, host groups), DHCP, and more. In Django mreg, these domains are intertwined in models and views both, making it hard to seperate concerns and rules.  As one example, HINFO is a data field on the host model instead of a first-class DNS record type.
- **Performance at scale.** Django ORM queries and Python runtime overhead limit throughput for large zone exports and bulk operations. Scale-out across multiple instances is unfeasible due to ID scheme and database coupling.
- **Type safety.** Python's dynamic types let invalid data slip through to the database. DNS names, CIDRs, serial numbers, and TTLs deserve compile-time enforcement.
- **Tight coupling.** Django model serializers mix persistence, validation, and HTTP concerns in one layer. Adding a new record type means touching models, serializers, views, and URL configs.
- **Seperation of concerns.** Django mreg has only limited support for multi-step operations that must succeed or fail atomically (e.g., creating a host with multiple IPs). This new implementation provides a storage trait layer with pluggable backends, allowing for atomic transactions across multiple entities and operations at the trait boudary layer.
- **Async work.** Background tasks (zone exports, reports, bulk imports) do not exist in Django mreg, and would probably end up having to rely on Celery, adding operational complexity due to python threading and process management. mreg-rust has a built-in task queue management through the pluggable storage backends.

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
Domain          Types, value objects, commands, validation. No I/O.
```

The storage layer is pluggable at startup (`MREG_STORAGE_BACKEND=auto|memory|postgres`). The in-memory backend enables the full test suite to run quickly without any external dependencies.

### How a request flows through the layers

Here is a concrete example: creating a label via `POST /inventory/labels`. It uses the DTO (data transfer object) pattern to separate API concerns from domain logic, and the command pattern to encapsulate validated operations.

**1. API handler** (`src/api/v1/labels.rs`) — Receives the HTTP request, deserializes the JSON body into a `CreateLabelRequest` DTO (a plain serde struct with `name: String, description: String`). Calls `into_command()` which converts the raw strings into validated domain types: `LabelName::new(self.name)?` validates the name (lowercase, no special characters). If validation fails, the handler returns a 400 immediately. The result is a `CreateLabel` command — a domain object that is guaranteed to carry valid data.

```rust
// API DTO → domain command (validation happens here)
fn into_command(self) -> Result<CreateLabel, AppError> {
    CreateLabel::new(LabelName::new(self.name)?, self.description)
}
```

**2. Service** (`src/services/labels.rs`) — The handler calls `label_service::create(store, audit, events, command)`. The service forwards the command to the storage trait, then records an audit event and emits a domain event. The service never inspects or transforms the data — it trusts the command is valid because the domain layer enforced that.

**3. Storage trait** (`src/storage/labels.rs`) — Defines `LabelStore::create_label(&self, command: CreateLabel) -> Result<Label, AppError>`. This trait is backend-neutral — callers don't know whether they're talking to PostgreSQL or an in-memory HashMap.

**4. Storage backend** (e.g., `src/storage/postgres/labels.rs`) — Inserts a row using Diesel, gets back a `LabelRow` (a Diesel model struct with raw database types), and calls `row.into_domain()` to convert it back to a domain `Label`.

**5. Database row → domain entity** (`src/db/models.rs`) — The `into_domain()` method re-validates data coming out of the database through the same newtype constructors:

```rust
impl LabelRow {
    pub fn into_domain(self) -> Result<Label, AppError> {
        Label::restore(
            self.id,
            LabelName::new(self.name)?,   // re-validated
            self.description,
            self.created_at,
            self.updated_at,
        )
    }
}
```

**6. API response** — The handler converts the domain `Label` to a `LabelResponse` DTO (extracting values via accessors like `label.name().as_str()`) and returns it as JSON.

### Type-driven validation at every boundary

A key design principle: **data is validated at every boundary, not just on input.** The newtype constructors (`LabelName::new()`, `Ttl::new()`, `VlanId::new()`, `SoaSeconds::new()`, etc.) are called both when parsing API requests and when reading rows from the database via `into_domain()`.

This means that if someone manually modifies a database value to something invalid — say, setting a VLAN ID to 99999 or a TTL to -1 — the system will return an error rather than silently propagating the corrupt value. The application never constructs a domain entity with invalid field values, regardless of where the data came from.

All newtype inner fields are private (e.g., `pub struct Ttl(u32)`, not `pub struct Ttl(pub u32)`), so the only way to obtain a value is through the validating constructor. Accessors like `as_u32()` and `as_i32()` expose the value read-only.

Django mreg, by contrast, trusts whatever the ORM loads from the database. If a column contains an out-of-range value, it propagates through serializers to API responses without complaint.

## Data model changes

### DNS records: one endpoint, not twelve

Django mreg has a separate model, serializer, view, and URL route for each record type (`/cnames/`, `/txts/`, `/mxs/`, `/srvs/`, ...). Adding a new type means touching all four layers.

mreg-rust has a single `POST /dns/records` endpoint that accepts a `type_name` field. 18 built-in types ship with RFC-aware validation schemas. Custom types can be registered at runtime and use RFC 3597 raw RDATA encoding.

### Hosts, networks, and IPs are inventory, not DNS

Django mreg models hosts, networks, and IP addresses as part of the DNS data model. mreg-rust treats them as inventory entities separate from DNS records. This allows for clearer separation of concerns and more flexible relationships (e.g. a host can have multiple IPs across different zones without being tied to a specific DNS record).

### HINFO is a record, not a host field

Django mreg stores HINFO as a field on the host model. mreg-rust treats it as a standard DNS record anchored to the host, just like MX or TXT.

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
- **Built-in export templates** for Kea DHCPv4/v6 and ISC DHCPd files, both full configs and host snippets.

See [dhcp-and-attachments.md](dhcp-and-attachments.md) for the full workflow.

## Export and import

### Exports

Django mreg uses custom Python scripts for zone file generation. mreg-rust uses MiniJinja templates with a well-defined context (zones, hosts, records, DHCP data). Nine built-in templates cover BIND zone files and Kea/ISC DHCP configs. Custom templates can be registered at runtime. See [export-templating.md](export-templating.md) for details.

Exports run asynchronously via the task queue. Results are stored and retrievable.

### Imports

Django mreg imports data via per-endpoint API calls. mreg-rust adds a staged
bulk import endpoint (`POST /workflows/imports`) that accepts a mixed-entity
JSON batch with forward references. Batches are executed by workers via
`POST /workflows/tasks/run-next`, then committed atomically or rolled back on
failure.

See [import-format.md](import-format.md) for the JSON contract and examples,
including execution flow and ordering constraints.

## Authentication

Django mreg uses Django REST framework token authentication.

mreg-rust supports two modes:

- **`none`** -- identity trusted from `X-Mreg-User`/`X-Mreg-Groups` headers (dev/test)
- **`scoped`** -- login with `scope:username` against configured backends (local users with Argon2id, LDAP bind, or remote JWT delegation), receiving an mreg-issued JWT with a namespace-aware principal such as `["mreg","local"] + "admin"`

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
