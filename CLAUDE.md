# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Rust reimplementation of the Django-based [mreg](https://github.com/unioslo/mreg) — a DNS and network inventory management REST API. Manages zones, hosts, DNS records (18 built-in types with RFC validation), networks, and related infrastructure. Uses Rust 2024 edition.

## Commands

```bash
cargo build                    # Build
cargo run                      # Run server (localhost:8080, Swagger at /swagger-ui/)
cargo test                     # Run tests (memory-only, postgres tests skipped)

# With PostgreSQL (required for full test suite):
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test --test dual_backend
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test --test postgres_storage
MREG_TEST_DATABASE_URL="..." cargo test test_name_here          # Single test
MREG_TEST_DATABASE_URL="..." cargo test test_name -- --nocapture # With output

# Database migrations (requires diesel_cli)
DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg" diesel migration run
```

## Architecture

Five-layer design with pluggable storage backends:

```
src/api/v1/        → Actix-web HTTP handlers, request/response DTOs, OpenAPI via utoipa
src/authn/         → Authentication providers (none/forward/LDAP), JWT issuing/validation
src/services/      → Audit recording + event emission on mutations, delegates to storage
src/storage/       → Storage trait definitions + two backend implementations
  storage/memory/  → In-memory HashMap backend (for tests)
  storage/postgres/→ Diesel ORM backend (for production)
src/domain/        → Value objects, entities, commands, validation (transport-agnostic)
src/db/            → Diesel schema.rs (auto-generated), models.rs (row types)
```

**Data flow per request:** API handler → parses request into domain command → calls service → service delegates to storage trait → backend executes → service records audit event + emits domain event → API handler maps to response DTO.

### Key design patterns

**Type-driven domain:** All domain invariants are value objects in `src/domain/types.rs` (DnsName, Hostname, ZoneName, LabelName, CidrValue, etc.). They validate and normalize on construction (e.g., DNS names lowercased, trailing dots stripped). Private fields, no mutation — only `new()`, `restore()`, and accessor methods. Custom Serialize/Deserialize impls that go through validation.

**Storage trait composition:** The `Storage` trait in `src/storage/mod.rs` aggregates ~18 subsystem store traits (LabelStore, ZoneStore, HostStore, RecordStore, etc.). Each store trait defines CRUD + listing with filtering/pagination. Both backends implement all traits. Backend selected at runtime via `MREG_STORAGE_BACKEND` env var (auto/memory/postgres).

**Atomic cascading side-effects:** Operations like record cleanup, zone serial bumps, and auto-created PTR records happen inside storage implementations, not orchestrated by services.

**Unified errors:** `AppError` enum in `src/errors.rs` maps variants to HTTP status codes (Validation→400, NotFound→404, Conflict→409, etc.). All errors serialize to `{ error: "kind", message: "details" }`.

**Cursor-based pagination:** `PageRequest`/`Page<T>` in `src/domain/pagination.rs`. The `page_response!` macro generates utoipa-visible wrappers for each entity type.

**Service-layer audit + events:** Audit recording and event emission are enforced at the service layer (`src/services/`), not inside storage backends. This guarantees every mutation is audited and emits a `DomainEvent` regardless of backend. Events are fire-and-forget via `EventSink` trait (`src/events/`), with webhook, AMQP, and Redis backends. AMQP and Redis are behind feature flags. See `docs/event-system.md`.

**Authentication:** Configurable via `MREG_AUTH_MODE` (none/scoped). In `none` mode, identity is trusted from `X-Mreg-User`/`X-Mreg-Groups` headers. In `scoped` mode, named auth scopes (local, remote, LDAP) provide `POST /api/v1/auth/login` with JWT issuance. Actix middleware validates bearer tokens and populates `PrincipalContext` in request extensions. `extract_principal()` reads from extensions first, falling back to headers in none mode. Token revocation is supported via `AuthSessionStore`. See `docs/authentication.md`.

**Structured logging:** Uses `tracing` with per-request spans containing `request_id`, `principal`, `http.method`, `http.target`, `http.status_code`. Service functions are instrumented with `#[tracing::instrument]` and `resource_kind` fields. Errors are logged automatically (WARN for 4xx, ERROR for 5xx). JSON output via `MREG_JSON_LOGS=true`. See `docs/logging.md`.

### Adding a new entity

Follow the existing pattern across all layers (use labels as the simplest example):
1. Domain entity + commands: `src/domain/foo.rs`
2. Storage trait: `src/storage/foo.rs`
3. Memory backend: `src/storage/memory/foo.rs`
4. Postgres backend: `src/storage/postgres/foo.rs`
5. Service: `src/services/foo.rs`
6. API handler: `src/api/v1/foo.rs`
7. Register in `src/api/mod.rs` (OpenAPI paths + schemas) and `src/api/v1/mod.rs` (routes)
8. Register store trait in `src/storage/mod.rs` and both backend mod.rs files
9. DB migration in `migrations/` (schema.rs auto-regenerates), model in `src/db/models.rs`

## Testing

**Dual-backend conformance tests** (`tests/dual_backend.rs`): The `dual_backend_test!` macro generates both `test_name::memory` and `test_name::postgres` variants from a single scenario function. Postgres tests are skipped when `MREG_TEST_DATABASE_URL` is not set.

**TestCtx** (`tests/common/mod.rs`): Provides namespaced test data (names, zones, CIDRs) so tests can run in parallel against a shared database without conflicts. Includes HTTP helpers (`get_json`, `post`, `patch`, `delete`) and seed methods (`seed_zone`, `seed_host`, `seed_network`).

**Other test files:** `api_contract_memory.rs` (API contract tests), `rr_validation.rs` (DNS record validation), `filter_*.rs` (filtering), `pagination.rs`, `sorting.rs`.

## Configuration

Environment variables (see `.env.example`). Key ones: `MREG_DATABASE_URL`, `MREG_TEST_DATABASE_URL`, `MREG_STORAGE_BACKEND` (auto/memory/postgres), `MREG_LISTEN`, `MREG_PORT`, `MREG_ALLOW_DEV_AUTHZ_BYPASS`. Full reference in `src/config.rs` and `docs/configuration.md`.

## Documentation

Detailed guides in `docs/` covering architecture, API differences from Django mreg, storage layer, type-driven design, pagination/filtering, DNS record standards, host policy, export templating, and more.
