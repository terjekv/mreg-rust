# Contributing to mreg-rust

A Rust reimplementation of [mreg](https://github.com/unioslo/mreg), a Django-based DNS and network inventory management REST API. This guide covers what you need to get started.

## Development Environment

**Rust toolchain:** Install via [rustup](https://rustup.rs/). The project uses edition 2024.

**PostgreSQL 15+:** Required for the full test suite. Quickest setup via Docker:

```bash
docker run -d --name mreg-postgres \
  -e POSTGRES_USER=mreg -e POSTGRES_PASSWORD=mreg -e POSTGRES_DB=mreg \
  -p 5433:5432 postgres:17

# Create the test database
psql "postgres://mreg:mreg@localhost:5433/mreg" -c "CREATE DATABASE mreg_test"
```

**diesel_cli:** Required for database migrations:

```bash
cargo install diesel_cli --no-default-features --features postgres
```

**Environment configuration:** Copy `.env.example` to `.env` and adjust values for your setup. Key variables are documented in `docs/configuration.md`.

**Run migrations and start the server:**

```bash
DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg" diesel migration run
cargo run
```

The API serves at `http://localhost:8080/api/v1/` with Swagger UI at `http://localhost:8080/swagger-ui/`.

## Running Tests

Memory-only tests require no external dependencies:

```bash
cargo test
```

Full test suite including PostgreSQL-backed tests:

```bash
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test
```

Targeted test suites:

```bash
MREG_TEST_DATABASE_URL="..." cargo test --test dual_backend       # Dual-backend conformance
MREG_TEST_DATABASE_URL="..." cargo test --test postgres_storage   # Postgres-specific semantics
cargo test --test rr_validation                                   # DNS record validation
cargo test --test api_contract_memory                             # API contract tests
```

Single test with output:

```bash
cargo test test_name -- --nocapture
```

Postgres-backed tests are automatically skipped when `MREG_TEST_DATABASE_URL` is not set.

## Code Organization

The codebase follows a five-layer architecture:

| Layer | Location | Role |
|-------|----------|------|
| API | `src/api/v1/` | Actix-web handlers, request/response DTOs, OpenAPI docs |
| Service | `src/services/` | Audit recording, event emission, delegates to storage |
| Storage | `src/storage/` | Trait definitions + memory and Postgres backends |
| Domain | `src/domain/` | Value objects, entities, commands, validation |
| Database | `src/db/` | Diesel schema and row types |

Request flow: API handler parses a domain command, calls the service layer, which delegates to a storage backend, records an audit event, and emits a domain event.

See `CLAUDE.md` for detailed design patterns (type-driven domain, storage trait composition, cascading side-effects, pagination, error handling).

## Adding a New Entity

Follow the 9-step checklist in `CLAUDE.md`. The **labels** entity is the simplest working example:

- Domain: `src/domain/labels.rs`
- Storage trait: `src/storage/labels.rs`
- Memory backend: `src/storage/memory/labels.rs`
- Postgres backend: `src/storage/postgres/labels.rs`
- Service: `src/services/labels.rs`
- API handler: `src/api/v1/labels.rs`

Every new entity needs a migration in `migrations/`, a model in `src/db/models.rs`, and registration in the relevant `mod.rs` files.

## Code Quality

**Linting and formatting are enforced in CI:**

```bash
cargo clippy --all-targets --all-features -- -D warnings   # Zero warnings required
cargo fmt                                                   # Standard Rust formatting
```

**Testing requirements:**

- All new entities need dual-backend test coverage using the `dual_backend_test!` macro in `tests/dual_backend.rs`. This generates both `test_name::memory` and `test_name::postgres` variants from a single scenario.
- Tests must pass both with and without `MREG_TEST_DATABASE_URL` set (postgres tests skip gracefully when unset).

**Style guidelines:**

- Keep source files under ~1,000 lines.
- Keep functions close to 30-40 lines unless they are data tables or test fixtures.

## Pull Requests

CI runs on every PR:

- **clippy** with `-D warnings` (zero warnings policy)
- **Full test suite** against both memory and PostgreSQL backends
- **Benchmark compilation** to catch build regressions

PR benchmarks run automatically via iai-callgrind and criterion. Regression thresholds:

- **iai-callgrind:** 3% (instruction-count based, deterministic)
- **criterion:** 8% (wall-clock based, noisier)

PRs that exceed these thresholds will fail the benchmark check.
