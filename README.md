# mreg-rust

A DNS and network inventory management API, reimplemented in Rust from the original Django-based [mreg](https://github.com/unioslo/mreg).

## Quick start

### Prerequisites

- Rust (edition 2024)
- PostgreSQL 15+ (or Docker)
- `diesel_cli` (`cargo install diesel_cli --no-default-features --features postgres`)

### Using Docker for PostgreSQL

```bash
docker run -d --name mreg-postgres \
  -e POSTGRES_USER=mreg -e POSTGRES_PASSWORD=mreg -e POSTGRES_DB=mreg \
  -p 5433:5432 postgres:17

# Create the test database
psql "postgres://mreg:mreg@localhost:5433/mreg" -c "CREATE DATABASE mreg_test"
```

### Configuration

Copy and edit the environment file:

```bash
cp .env.docker.local .env
```

See [docs/configuration.md](docs/configuration.md) for all environment variables.

For scoped authentication, copy the scope registry template too:

```bash
cp auth-scopes.example.json auth-scopes.local.json
```

### Run migrations and start

```bash
DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg" diesel migration run
cargo run
```

The API is available at `http://localhost:8080/api/v1/`. Swagger UI is at `http://localhost:8080/swagger-ui/`.

### Run tests

```bash
# Full test suite with PostgreSQL-backed coverage
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test

# Targeted PostgreSQL-only semantics
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test --test postgres_storage

# Dual-backend conformance only
MREG_TEST_DATABASE_URL="postgres://mreg:mreg@localhost:5433/mreg_test" cargo test --test dual_backend
```

When you reuse a shared remote PostgreSQL database for test runs, reset it before a dedicated run so old state and old migrations do not accumulate. A simple pattern is:

```bash
psql "$MREG_DATABASE_URL" -c "DROP DATABASE IF EXISTS mreg_test WITH (FORCE);"
psql "$MREG_DATABASE_URL" -c "CREATE DATABASE mreg_test;"
```

### Query-budget regression tests

PostgreSQL-backed performance regression tests use Diesel connection instrumentation to capture the SQL statements executed for a single request. The shared test harness exposes this via `TestCtx::get_json_with_query_capture(...)`, and the database layer records per-request query snapshots through `src/db/mod.rs`.

Use this for hot endpoints where N+1 regressions matter, especially rich inventory reads. Prefer:

- upper bounds on total query count
- assertions that specific attachment/policy child-table queries only happen once per request

Avoid exact fragile budgets for broad CRUD coverage. Keep query-budget tests focused on endpoints where batching is part of the contract.

## Architecture

- **API layer** (`src/api/`): Actix-web handlers with OpenAPI documentation via utoipa
- **Domain layer** (`src/domain/`): type-safe value objects, validation, filters, pagination
- **Service layer** (`src/services/`): thin pass-through to storage traits
- **Storage layer** (`src/storage/`): pluggable backends (in-memory for tests, PostgreSQL for production)
- **Database layer** (`src/db/`): Diesel ORM with generated schema and row types

See [docs/architecture.md](docs/architecture.md) for the full design.

## DNS record support

18 built-in record types with RFC-aware validation: A, AAAA, NS, PTR, CNAME, MX, TXT, SRV, NAPTR, SSHFP, LOC, HINFO, DS, DNSKEY, CAA, TLSA, SVCB, HTTPS.

Runtime-defined record types with RFC 3597 raw RDATA support for any DNS type.

See [docs/rr-standards.md](docs/rr-standards.md) for validation rules and RFC references.

## Documentation

| Document | Description |
|----------|-------------|
| [architecture.md](docs/architecture.md) | System design and module structure |
| [api-differences.md](docs/api-differences.md) | How this API differs from old Django mreg |
| [api-compatibility.md](docs/api-compatibility.md) | Compatibility layer for old mreg clients |
| [authentication.md](docs/authentication.md) | Authentication modes, login flow, and bearer-token behavior |
| [authorization.md](docs/authorization.md) | Authorization model, action wiring, and Treetop integration |
| [authz-action-matrix.md](docs/authz-action-matrix.md) | Detailed authorization actions, resources, and attrs |
| [configuration.md](docs/configuration.md) | Environment variable reference |
| [domain-catalog.md](docs/domain-catalog.md) | All domain types and commands |
| [export-templating.md](docs/export-templating.md) | MiniJinja export template system |
| [host-policy.md](docs/host-policy.md) | Host policy atoms and roles |
| [import-format.md](docs/import-format.md) | Bulk import JSON format |
| [migration-guide.md](docs/migration-guide.md) | How to migrate from Django mreg |
| [migration-backlog.md](docs/migration-backlog.md) | Deferred migration work |
| [pagination-sort-filter.md](docs/pagination-sort-filter.md) | Pagination, sorting, and filtering API |
| [rr-standards.md](docs/rr-standards.md) | DNS record type validation rules |
| [storage-layer.md](docs/storage-layer.md) | Storage architecture and backend details |
| [type-driven-design.md](docs/type-driven-design.md) | Value object design philosophy |
| [event-system.md](docs/event-system.md) | Domain event sinks (webhook, AMQP, Redis) |
| [logging.md](docs/logging.md) | Structured logging, log levels, JSON format |
| [wildcard-dns.md](docs/wildcard-dns.md) | Wildcard DNS record support |

## License

TBD
