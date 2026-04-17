# Architecture

## Goal

`mreg-rust` is a fresh Rust implementation of the core `mreg` DNS and network inventory API.
The service uses `Actix Web` for HTTP, a storage facade for persistence access, `Diesel` with PostgreSQL
for the primary production adapter, `MiniJinja` for export templating, `utoipa` for OpenAPI documentation,
and `treetop-rest` for policy evaluation.

## System Boundaries

- This service owns domain data, validation, transactions, import/export workflows, and audit history.
- `treetop-rest` owns authorization policy evaluation.
- PostgreSQL is the canonical system of record for production.
- Background work is executed by database-backed workers using row locking.
- The rest of the application talks to storage through subsystem traits instead of raw database calls.
- Cascading side-effects (record cleanup, serial bumps, A/AAAA/PTR auto-creation) are handled atomically inside the storage layer, not orchestrated by the service layer.

## Top-Level Modules

- `api`: HTTP handlers, route composition, OpenAPI spec generation (utoipa), and Swagger UI.
- `domain`: transport-agnostic domain types, validation contracts, filters, and pagination types.
- `services`: thin application use cases forwarding typed commands to storage traits.
- `storage`: backend-neutral storage facade, runtime backend selection, and trait definitions.
  - `storage::memory`: in-memory backend for tests and lightweight development.
  - `storage::postgres`: PostgreSQL backend split into per-store modules (labels, nameservers, zones, hosts, networks, records, ancillary, tasks, imports, exports, audit, helpers).
- `db`: Diesel connection pool, generated schema (`schema.rs`), row types (`models.rs`), and migration runner.
- `authz`: adapter for `treetop-rest` request/response mapping with principal extraction and permission checking.
- `audit`: history event types and audit trail capture.

## Runtime Flow

1. HTTP request enters `api`.
2. Request context is normalized into domain input.
3. Strings and primitives are converted into typed value objects and command types.
4. If required, resource facts are assembled and sent to `authz`.
5. Domain services call the `storage` facade rather than `Diesel` directly.
6. The active storage backend performs persistence operations with atomic cascading.
7. Mutating operations emit audit history events within the same storage transaction.
8. Long-running work is represented as a `task` and processed asynchronously.

## Storage Principles

- Prefer first-class typed storage for hosts, networks, IP addresses, zones, and delegations.
- Use capability-oriented traits instead of generic CRUD repositories.
- Handle cascading side-effects atomically within storage methods (e.g., host deletion cascades to records, IP assignment creates A/AAAA and PTR records, zone serial bumps on any record mutation).
- Keep PostgreSQL-native optimizations in the PostgreSQL adapter.
- Use the in-memory backend for targeted tests and lightweight development, not as a semantic substitute for PostgreSQL.
- Preserve enough permission-relevant facts in the domain model for external policy checks.
- Keep core invariants in opaque value objects with private fields and explicit constructors.
- All list operations support cursor-based pagination with configurable sorting and entity-specific filtering at the storage layer.

## API Surface

- REST endpoints under `/api/v1/` with JSON request/response bodies.
- OpenAPI 3.x spec auto-generated via `utoipa` and served at `/api-docs/openapi.json`.
- Swagger UI at `/swagger-ui/`.
- Consistent paginated response shape: `{ items, total, next_cursor }`.
- Entity-specific query parameters for filtering and sorting.

## Compatibility Strategy

- Preserve the main conceptual nouns and workflows from upstream `mreg`.
- Maintain a semantically compatible `/api/v1` surface.
- Favor Rust-native internal structure over Django model mirroring.
- Defer legacy import tooling until the fresh-install MVP is stable.
