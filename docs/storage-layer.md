# Storage Layer

## Why This Exists

The storage layer is the boundary between application logic and backend-specific persistence code.

Its job is to:

- keep HTTP handlers and most services free of Diesel and raw SQL
- centralize transactions and cascading side-effects
- isolate PostgreSQL-specific query logic
- support a fast in-memory backend for tests and lightweight development
- provide a consistent capability-oriented interface to the rest of the application

## Core Design

The storage API is split into resource-oriented traits, not a single generic repository.

Examples:

- `HostStore`
- `AttachmentStore`
- `RecordStore`
- `NetworkStore`
- `CommunityStore`
- `TaskStore`
- `ImportStore`
- `ExportStore`
- `AuthSessionStore`
- `AuditStore`

These traits are collected behind the umbrella `Storage` trait in `src/storage/mod.rs`.

## Service Boundary vs Storage Boundary

Handlers should not talk to write-capable storage traits directly.

Current model:

- handlers call `Services` for domain reads and writes
- services call the storage traits
- `ReadableStorage` is intentionally narrow and only exposes backend diagnostics:
  - backend kind
  - capability summary
  - health status

This is deliberate. The read path is kept inside the service facade so handlers cannot bypass audit/event wiring or drift into backend-specific coupling.

## What Storage Owns

The storage layer owns:

- persistence and lookup
- transactions
- list/filter/sort execution
- backend-specific query plans and batching
- cascading side-effects
- task persistence and claiming
- import/export persistence and rendering support
- auth-session revocation persistence

The storage layer does not own:

- HTTP request parsing
- authentication or authorization decisions
- audit/event orchestration policy
- OpenAPI or route structure

## Capability-Oriented Traits

The current storage facade includes:

- `LabelStore`
- `NameServerStore`
- `ZoneStore`
- `NetworkStore`
- `HostStore`
- `AttachmentStore`
- `HostContactStore`
- `HostGroupStore`
- `BacnetStore`
- `PtrOverrideStore`
- `NetworkPolicyStore`
- `CommunityStore`
- `AttachmentCommunityAssignmentStore`
- `HostCommunityAssignmentStore`
- `HostPolicyStore`
- `TaskStore`
- `ImportStore`
- `ExportStore`
- `RecordStore`
- `AuditStore`
- `AuthSessionStore`

Most list methods accept a `PageRequest`. Resources with richer query requirements also accept typed filter objects from `src/domain/filters/`.

## Cascading Side-Effects

Storage methods are responsible for persistence side-effects that must be atomic.

Examples:

- host deletion cascades to dependent records and address state
- host rename updates record ownership fields
- IP assignment may auto-create A/AAAA and PTR records
- IP unassignment may auto-remove generated A/AAAA and PTR records
- record create/update/delete bumps zone serials
- zone creation or nameserver updates synchronize derived NS records
- attachment and policy mutations maintain dependent relationship state

In memory, this is done under a single state lock. In PostgreSQL, this is done inside transactions.

## Backends

### Memory backend

Location:

- `src/storage/memory/`

Characteristics:

- not persistent
- all state stored in an in-memory `MemoryState`
- optimized for tests and local development
- pagination, sorting, and filtering usually happen in Rust
- good for handler and orchestration tests

This backend intentionally prioritizes simplicity over production-grade semantics.

### PostgreSQL backend

Location:

- `src/storage/postgres/`

Characteristics:

- persistent
- transactionally strong
- uses PostgreSQL-native network operators and indexing
- mixes Diesel query builder and `sql_query` where PostgreSQL-specific operators are needed
- supports production task-claiming semantics with `FOR UPDATE SKIP LOCKED`

Current layout is split by store area:

```text
src/storage/postgres/
├── mod.rs
├── attachments.rs
├── audit.rs
├── auth_sessions.rs
├── bacnet_ids.rs
├── communities.rs
├── exports.rs
├── host_community_assignments.rs
├── host_contacts.rs
├── host_groups.rs
├── host_policy.rs
├── hosts.rs
├── imports.rs
├── labels.rs
├── nameservers.rs
├── network_policies.rs
├── networks.rs
├── ptr_overrides.rs
├── records.rs
├── tasks.rs
├── helpers/
│   ├── dynamic_query.rs
│   ├── pagination.rs
│   ├── record_owner.rs
│   ├── record_types.rs
│   └── zone_serial.rs
└── zones/
    ├── delegations.rs
    ├── forward.rs
    ├── reverse.rs
    └── mod.rs
```

## PostgreSQL-Specific Responsibilities

The PostgreSQL adapter is where backend-specific optimizations belong.

Examples:

- CIDR and INET containment queries
- batched association loading to avoid N+1 paths
- dynamic SQL for filtered list endpoints
- count queries and `LIMIT n+1` pagination behavior
- zone serial helpers
- record-owner resolution helpers
- auth-session cleanup

These optimizations should not leak into handlers or services.

## Read Models and Rich Endpoints

Some inventory endpoints return richer detail views, such as host and network detail responses.

Those endpoints still go through services, but the storage layer is responsible for:

- batched association loading
- minimizing repeated queries
- returning data that is internally consistent for the chosen backend

PostgreSQL query-budget tests exist specifically to prevent regressions on these hot paths.

## Auth Session Persistence

`AuthSessionStore` persists bearer-token revocation state.

It currently supports:

- revoking the current token
- revoking all tokens for a principal before a cutoff
- checking whether a token fingerprint is revoked
- pruning expired revocation records

The application starts a background task that prunes expired revoked-token rows periodically.

## Imports, Exports, and Tasks

The storage layer also owns the durable workflow state for:

- atomic import batches
- export templates and export runs
- background tasks and task claiming

This keeps workflow semantics close to the transaction boundary and avoids duplicating persistence logic in handlers.

## Testing Guidance

Use memory-backed tests for:

- handler tests
- pure orchestration tests
- API contract checks that do not depend on PostgreSQL semantics

Use dual-backend tests for:

- shared API behavior that both backends intentionally support
- conflict handling
- import atomicity
- core sorting/filtering contract

Use PostgreSQL-backed tests for:

- transaction boundaries
- task claiming and concurrency
- PostgreSQL-native network semantics
- persistence across fresh app/state instances
- query-budget regression coverage on hot endpoints

### Running PostgreSQL tests

```bash
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test
```

When `MREG_TEST_DATABASE_URL` is not set, PostgreSQL-only tests skip and the rest of the suite still runs.

## Query-Budget Tests

The PostgreSQL path supports request-scoped SQL capture through Diesel instrumentation in `src/db/mod.rs`.

The test harness in `tests/common/mod.rs` uses that to assert:

- total SQL statements for one request
- normalized statement fingerprints
- that known batched child loads execute once rather than once per row

These tests are the main guard against reintroducing N+1 behavior on rich endpoints.

## Important Non-Goal

The storage abstraction does not promise perfect backend equivalence for every edge case.

The project is designed around PostgreSQL as the canonical production backend. Memory exists to support fast tests and local development, not to replace PostgreSQL as the semantic reference.

## Related Documents

- [architecture.md](architecture.md)
- [authentication.md](authentication.md)
- [authorization.md](authorization.md)
- [configuration.md](configuration.md)
