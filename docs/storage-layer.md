# Storage Layer

## Why This Exists

The service has an explicit storage boundary between application/domain logic and backend-specific persistence code. The goal is to:

- keep HTTP handlers and domain services free of Diesel and raw SQL
- make unit and orchestration testing easier
- isolate PostgreSQL-specific optimizations in one adapter
- allow lightweight in-memory execution for tests and development
- handle cascading side-effects atomically within storage methods

## Design Principle

Use capability-oriented storage traits instead of a giant generic repository abstraction.

The storage facade is split by resource, which maps cleanly into the higher-level DNS, inventory, and policy domains:

- `LabelStore` — label CRUD with update support
- `NameServerStore` — nameserver CRUD with update support
- `ZoneStore` — forward/reverse zone CRUD, delegation management, serial bumping
- `NetworkStore` — network CRUD, excluded ranges
- `HostStore` — host CRUD with cascading record cleanup, IP assignment/unassignment with auto A/AAAA/PTR creation
- `HostContactStore` — host contact CRUD
- `HostGroupStore` — host group CRUD
- `BacnetStore` — BACnet ID assignments
- `PtrOverrideStore` — PTR overrides
- `NetworkPolicyStore` — network policy CRUD
- `CommunityStore` — community CRUD
- `HostCommunityAssignmentStore` — host-community assignments
- `TaskStore` — task queue with claiming semantics
- `ImportStore` — atomic batch import
- `ExportStore` — template management and export rendering
- `RecordStore` — record type definitions, RRSet management, record CRUD with zone serial bumping, owner-based queries and cascading
- `AuditStore` — history event recording and retrieval
- `Storage` — umbrella trait for backend metadata, health, capabilities, and access to all subsystem traits

All list methods accept `PageRequest` for cursor-based pagination with sorting. Methods for hosts, networks, records, and the inventory/policy resources also accept entity-specific filter parameters.

## Cascading Side-Effects

The storage layer handles cascading operations atomically (within a single lock in memory, within a transaction in PostgreSQL):

- **Host deletion** cascades to: all records owned by the host, IP address cleanup, zone serial bump
- **Host rename** cascades to: owner_name/anchor_name updates on all associated records and RRsets
- **IP assignment** auto-creates: A or AAAA record, PTR record in matching reverse zone
- **IP unassignment** auto-deletes: matching A/AAAA record and PTR record
- **Record create/update/delete** auto-bumps: zone serial number
- **Zone creation** auto-creates: NS records for each nameserver
- **Zone nameserver update** auto-syncs: deletes old NS records, creates new ones
- All mutations record audit events

## Current Backends

### `memory` (`src/storage/memory.rs`)

Intended for unit tests, lightweight development, and scaffolding.

Characteristics:

- not persistent
- all state under a single `RwLock<MemoryState>` for atomic multi-collection operations
- no PostgreSQL-native network semantics
- pagination, sorting, and filtering applied in Rust

### `postgres` (`src/storage/postgres/`)

Intended for production, integration tests, and realistic development.

Split into per-store modules:

```
src/storage/postgres/
├── mod.rs           PostgresStorage struct + Storage impl
├── helpers.rs       shared utilities (serial bump, record owner resolution, etc.)
├── labels.rs        LabelStore (Diesel DSL)
├── nameservers.rs   NameServerStore (Diesel DSL)
├── zones.rs         ZoneStore (mixed Diesel DSL + sql_query)
├── networks.rs      NetworkStore (sql_query for INET/CIDR operations)
├── hosts.rs         HostStore (sql_query for INET joins)
├── ancillary.rs     AncillaryStore (sql_query with JOINs)
├── records.rs       RecordStore (mixed Diesel DSL + sql_query)
├── tasks.rs         TaskStore (Diesel DSL + FOR UPDATE SKIP LOCKED)
├── imports.rs       ImportStore (mixed, dynamic entity processing)
├── exports.rs       ExportStore (mixed)
└── audit.rs         AuditStore (Diesel DSL)
```

Characteristics:

- persistent with strong transactions
- PostgreSQL-native network types and indexing
- `FOR UPDATE SKIP LOCKED` worker semantics
- Diesel typed query builder for simple CRUD, raw `sql_query` for Postgres-specific operators (CIDR containment, INET, trigram search)

## Important Non-Goal

This abstraction does **not** promise backend-equivalent semantics for all features. The MVP is designed around PostgreSQL as the canonical backend.

PostgreSQL-specific behavior that matters:

- `inet` and `cidr` types for IP/network operations
- `jsonb` for record data and task payloads
- GIN/GiST/trigram indexes for full-text search
- `FOR UPDATE SKIP LOCKED` for task claiming
- ICU collation support (case-insensitivity enforced by application, not database)

## Testing Guidance

Use the in-memory backend for:

- handler tests
- orchestration/unit tests
- pagination, sorting, and filtering tests
- import/export wiring tests
- DNS record lifecycle tests

Use shared dual-backend conformance tests for:

- backend-neutral API behavior that both backends intentionally support
- uniqueness/conflict handling
- import atomicity and failed-batch persistence
- shared network containment/allocation rules
- task claiming state progression

Use PostgreSQL integration tests for:

- transaction boundaries
- `FOR UPDATE SKIP LOCKED` worker task claiming
- PostgreSQL-native network/query semantics
- query correctness with real data types
- query-budget regressions on hot endpoints

### Running PostgreSQL tests

```
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test
```

When `MREG_TEST_DATABASE_URL` is not set, PostgreSQL tests skip and the rest of the test suite runs normally.

### Query-budget tests

The PostgreSQL path supports request-scoped SQL capture through Diesel instrumentation in `src/db/mod.rs`. The shared integration-test harness exposes that capture via `tests/common/mod.rs`, which lets tests assert:

- total SQL statements executed for a request
- how many times a normalized statement fingerprint appears
- that known batched child lookups only execute once

Use these tests for the endpoints most at risk of N+1 regressions, such as rich inventory detail responses. For example:

- `GET /inventory/hosts/{name}`
- `GET /inventory/networks/{cidr}`

Prefer stable upper bounds and single-query expectations for known batched subloads over brittle exact counts for every endpoint.

## Code Layout

- `src/storage/mod.rs`: trait definitions, backend metadata, factory
- `src/storage/memory.rs`: in-memory implementation
- `src/storage/postgres/`: PostgreSQL adapter (13 files)
- `src/db/mod.rs`: connection pool and migration runner
- `src/db/schema.rs`: Diesel-generated table definitions
- `src/db/models.rs`: row types with domain conversion methods
