# Architecture

## Goal

`mreg-rust` is the Rust implementation of the `mreg` DNS and network inventory service. The application is structured around a narrow request pipeline:

1. HTTP and middleware
2. authentication
3. authorization
4. domain validation
5. service-layer orchestration
6. storage-backed transactions
7. audit and event emission

The design is intentionally pragmatic:

- Actix Web handles HTTP, middleware, and application wiring
- domain types own validation and normalization
- services provide one place for application use cases and audit/event wiring
- storage traits isolate persistence concerns and backend-specific query logic
- PostgreSQL is the canonical production backend
- Treetop is the external authorization engine

## System Boundaries

This service owns:

- HTTP API behavior under `/api/v1`
- authentication and local bearer-token issuance/validation
- domain validation and normalization
- persistence and transactional side-effects
- import/export workflows
- task queue state
- audit history
- domain event emission

This service does not own:

- external policy decisions when Treetop is configured
- external identity systems used by scoped auth backends such as LDAP or remote SSO
- live DHCP lease state

## Architectural Layers

### HTTP and middleware

The HTTP surface is built with Actix Web.

Key responsibilities:

- route registration
- request parsing
- OpenAPI generation
- request ID and tracing setup
- authentication middleware

Relevant modules:

- `src/api/`
- `src/middleware/`
- `src/lib.rs`

### Authentication

Authentication resolves the caller into a canonical `PrincipalContext` before handlers execute protected operations.

Current model:

- `auth_mode=none`
  - trusts `X-Mreg-User` and `X-Mreg-Groups`
  - intended for tests and trusted local development only
- `auth_mode=scoped`
  - authenticates through one configured auth scope
  - supports `local`, `ldap`, and `remote` scope backends
  - always issues and validates mreg-local bearer tokens

Important implementation points:

- `middleware::Authn` enforces bearer authentication for protected endpoints
- resolved principals are attached to request extensions
- handlers and the authorization layer consume the resolved principal, not raw headers

Relevant modules:

- `src/authn/`
- `src/middleware/authn.rs`

See [authentication.md](authentication.md) for the full flow.

### Authorization

Authorization happens after authentication has resolved the caller.

Current model:

- if `MREG_TREETOP_URL` is set, authorization is delegated to Treetop
- if no Treetop URL is configured and `MREG_ALLOW_DEV_AUTHZ_BYPASS=true`, all actions are allowed
- otherwise requests are denied

Handlers build explicit authorization requests using:

- principal
- action
- resource kind
- resource id
- resource attrs

Relevant modules:

- `src/authz/`
- `src/api/v1/authz.rs`

See [authorization.md](authorization.md) and [authz-action-matrix.md](authz-action-matrix.md).

### Domain layer

The domain layer is transport-agnostic. It owns:

- value objects and parsing
- validation rules
- filter types
- pagination and sorting contracts
- command and response-neutral aggregate types

Examples:

- DNS names, TTLs, CIDRs, MAC addresses
- import/export command envelopes
- host attachments and DHCP identifiers

Relevant module:

- `src/domain/`

### Service layer

The service layer is intentionally thin, but it is still a real boundary.

It owns:

- application use-case composition
- audit-event recording
- domain event emission
- keeping handlers away from write-capable storage traits

Handlers should read and write domain data through `Services`, not by reaching directly into backend-specific storage.

Relevant module:

- `src/services/`

### Storage layer

The storage layer is the persistence boundary. It is split into capability-oriented traits rather than a single generic repository.

It owns:

- transactional persistence
- backend-specific query logic
- list/filter/sort execution
- cascading side-effects
- task queue persistence
- import/export persistence
- auth-session revocation state

Current backends:

- `memory`
  - fast test/development backend
  - implemented under `src/storage/memory/`
- `postgres`
  - canonical production backend
  - implemented under `src/storage/postgres/`

`ReadableStorage` is intentionally narrow. It is used for backend diagnostics such as health and capabilities, not general domain reads.

Relevant modules:

- `src/storage/`
- `src/db/`

See [storage-layer.md](storage-layer.md) for the detailed storage model.

### Audit and events

Mutations generate audit history and may emit domain events.

Current model:

- services record history through the `AuditStore`
- services emit domain events through `EventSinkClient`
- audit persistence failure is logged, not silently swallowed
- event delivery supports multiple sinks

Relevant modules:

- `src/audit/`
- `src/events/`

### Workers and tasks

Long-running or asynchronous workflows are represented as tasks stored in the database/backend and processed by workers.

Current model:

- tasks are created and persisted through the task store
- PostgreSQL uses `FOR UPDATE SKIP LOCKED` semantics for safe concurrent claiming
- workers operate on persisted task state rather than in-memory queues

Relevant modules:

- `src/tasks/`
- `src/workers/`
- `src/storage/tasks.rs`

See [task-system.md](task-system.md) for the operational task lifecycle,
worker execution model, and endpoint semantics.

## Runtime Request Flow

For a typical protected request:

1. Actix receives the HTTP request.
2. request ID and root-span middleware attach tracing context.
3. authn middleware checks whether the path is exempt.
4. if protected, authn resolves the principal from a bearer token or, in `none` mode, from trusted headers.
5. the handler parses JSON/query/path input into domain commands and value objects.
6. the handler builds one or more authorization requests.
7. authorization is evaluated through Treetop, bypass mode, or deny mode.
8. the handler calls the appropriate service.
9. the service calls the storage facade.
10. the active backend performs the mutation or read, including transactional side-effects.
11. the service records audit history and emits any domain events.
12. the handler returns the HTTP response.

## Domain Boundaries in the API

The current `/api/v1` surface is grouped by domain:

- `dns`
  - nameservers
  - forward/reverse zones
  - delegations
  - records and RRsets
  - PTR overrides
- `inventory`
  - hosts
  - attachments
  - IP addresses
  - networks
  - labels
  - host contacts
  - host groups
  - BACnet assignments
- `policy`
  - network policies
  - communities
  - attachment-community assignments
  - host-policy atoms and roles
- `auth`
  - login
  - me
  - logout
  - logout-all
- `system`
  - health
  - version
  - status
  - history
- `workflows`
  - imports
  - exports
  - tasks

This grouping is organizational and API-facing. It is not a microservice split.

## Top-Level Modules

- `api`
  - route composition, handlers, OpenAPI metadata
- `authn`
  - scoped login backends, local JWT issuance/validation, principal resolution
- `authz`
  - Treetop request building and permission checks
- `domain`
  - value objects, commands, filters, pagination, import/export types
- `services`
  - use-case facade with audit/event wiring
- `storage`
  - trait-based persistence facade and runtime backend selection
- `db`
  - PostgreSQL connection pool, migrations, generated schema
- `events`
  - domain event sinks and emission
- `audit`
  - audit/history event types
- `middleware`
  - authn, request ID, tracing span integration
- `tasks`
  - task-domain types
- `workers`
  - background task execution

## Backend Strategy

PostgreSQL is the authoritative production target.

That has a few consequences:

- some semantics are intentionally PostgreSQL-first
- memory is used for speed and ergonomics in tests, not as a perfect semantic twin
- dual-backend tests cover the shared contract
- PostgreSQL-specific tests cover transactionality, query behavior, and performance-sensitive paths

## Related Documents

- [authentication.md](authentication.md)
- [authorization.md](authorization.md)
- [storage-layer.md](storage-layer.md)
- [configuration.md](configuration.md)
- [domain-catalog.md](domain-catalog.md)
