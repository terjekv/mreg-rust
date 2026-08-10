# Configuration

All configuration is via environment variables. A `.env.example` file is provided as a template.

For authentication flow and endpoint behavior, see [authentication.md](authentication.md). For authorization behavior and Treetop integration, see [authorization.md](authorization.md).

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MREG_LISTEN` | `127.0.0.1` | Bind address for the HTTP server |
| `MREG_PORT` | `8080` | HTTP port |
| `MREG_STORAGE_BACKEND` | `auto` | Storage backend: `auto`, `memory`, or `postgres`. `auto` selects `postgres` if `MREG_DATABASE_URL` is set, otherwise `memory`. |
| `MREG_JSON_PAYLOAD_LIMIT_BYTES` | `1048576` | Maximum accepted JSON request body size in bytes. Requests above this limit are rejected with `413 Payload Too Large`. |
| `MREG_DATABASE_URL` | — | PostgreSQL connection string (e.g., `postgres://mreg:mreg@localhost:5432/mreg`) |
| `MREG_TEST_DATABASE_URL` | — | Separate PostgreSQL connection for integration tests. When not set, PostgreSQL tests are skipped. |
| `MREG_RUN_MIGRATIONS` | `true` | Run Diesel migrations on startup |
| `MREG_JSON_LOGS` | `false` | Use JSON-structured logging (for production log aggregators) |
| `MREG_WORKERS` | CPU count | Number of Actix-web worker threads |
| `MREG_TREETOP_URL` | — | Treetop authorization service endpoint (e.g., `http://localhost:9999`). When not set, authorization is bypassed or denied depending on `MREG_ALLOW_DEV_AUTHZ_BYPASS`. |
| `MREG_TREETOP_TIMEOUT_MS` | `1500` | Timeout for Treetop authorization requests in milliseconds |
| `MREG_ALLOW_DEV_AUTHZ_BYPASS` | `false` | When `true` and no Treetop URL is configured, all authorization checks are allowed (development mode) |
| `MREG_AUTH_MODE` | `none` | Authentication mode: `none` or `scoped` |
| `MREG_AUTH_TOKEN_TTL_SECONDS` | `3600` | Maximum lifetime for mreg-issued access tokens in `scoped` mode |
| `MREG_AUTH_JWT_SIGNING_KEY` | — | HMAC secret used to sign and validate mreg-issued access tokens in `scoped` mode |
| `MREG_AUTH_JWT_ISSUER` | `mreg-rust` | Issuer claim for mreg-issued access tokens |
| `MREG_AUTH_CONFIG_PATH` | — | Path to the TOML authentication provider registry used in `scoped` mode |
| `MREG_EVENT_WEBHOOK_URL` | — | URL to POST domain events to as JSON. See [event-system.md](event-system.md). |
| `MREG_EVENT_WEBHOOK_TIMEOUT_MS` | `5000` | Timeout for webhook HTTP requests in milliseconds |
| `MREG_EVENT_AMQP_URL` | — | AMQP connection URL for event publishing (requires `amqp` feature) |
| `MREG_EVENT_AMQP_EXCHANGE` | `mreg.events` | AMQP exchange name (topic type, durable) |
| `MREG_EVENT_REDIS_URL` | — | Redis connection URL for event streaming (requires `redis` feature) |
| `MREG_EVENT_REDIS_STREAM` | `mreg:events` | Redis Stream key for event delivery |
| `MREG_DHCP_AUTO_V4_CLIENT_ID` | `false` | Auto-create a `client_id` DHCP identifier from the attachment MAC address when an IPv4 IP is assigned |
| `MREG_DHCP_AUTO_V6_DUID_LL` | `false` | Auto-create a `duid_ll` DHCP identifier from the attachment MAC address when an IPv6 IP is assigned |

## Environment Files

| File | Purpose |
|------|---------|
| `.env.example` | Template with default values for local development |
| `.env.docker.local` | Pre-configured for the Docker PostgreSQL container on port 5433 |

## Storage Backends

### Memory (development/testing)

No database needed. All data lives in memory and is lost on restart.

```
MREG_STORAGE_BACKEND=memory
```

### PostgreSQL (production)

Requires a running PostgreSQL instance with the `pgcrypto` and `pg_trgm` extensions available.

```
MREG_STORAGE_BACKEND=auto
MREG_DATABASE_URL=postgres://mreg:mreg@localhost:5432/mreg
MREG_RUN_MIGRATIONS=true
```

Case-insensitive behavior is handled by the application layer (all domain types normalize to lowercase). The database uses plain `TEXT` columns.

## Authorization Modes

| Configuration | Behavior |
|---------------|----------|
| `MREG_TREETOP_URL` set | All requests authorized via Treetop |
| No URL + `MREG_ALLOW_DEV_AUTHZ_BYPASS=true` | All requests allowed (dev mode) |
| No URL + bypass disabled | All requests denied |

## Authentication Modes

Authentication resolves the request principal before authorization runs.

| Mode | Behavior |
|------|----------|
| `none` | Trust `X-Mreg-User` and `X-Mreg-Groups` headers directly. Intended for tests and local development only. |
| `scoped` | `POST /api/v2/auth/login` authenticates against one configured auth scope and always returns an mreg-issued JWT access token. |

In `scoped` mode, protected endpoints require `Authorization: Bearer <token>`. `X-Mreg-User` and `X-Mreg-Groups` are ignored for identity resolution in that mode.

`GET /api/v2/system/health` and `GET /api/v2/system/version` remain unauthenticated.

## Authentication Providers

In `scoped` mode, mreg-rust loads one or more authentication providers from
`MREG_AUTH_CONFIG_PATH`.

Supported scope kinds:

- `local`
- `ldap`
- `remote`

Each provider has a unique startup-defined `scope`. Clients send `identity_scope`
and `username` as separate login fields.

Authenticated identity is namespace-aware:

- principal id is the raw username
- principal namespace is `["mreg", scope]`
- group ids are raw group names
- group namespace is `["mreg", scope]`
- the stable serialized principal key is `mreg::<scope>::<username>`

Example provider registry:

```toml
[[providers]]
scope = "local"
kind = "local"

[[providers.users]]
username = "admin"
password_hash = "$argon2id$v=19$m=19456,t=2,p=1$..."
groups = ["ops", "net"]
```

See [../auth-providers.example.toml](../auth-providers.example.toml) for a fuller example.

### Local scopes

`local` providers define static users directly in the provider file:

- `username`
- `password_hash`
- `groups`

Passwords use Argon2id PHC strings.

### LDAP scopes

`ldap` scopes define:

- `url`
- `connect_timeout_seconds` and `operation_timeout_seconds`
- `user_base_dn`, `user_filter`, and `user_scope`
- `username_attribute` and `subject_attribute`
- optional `display_name_attribute` and `email_attribute`
- `group_attributes`, `group_rules`, and optional `group_filters`
- optional paired `bind_dn` and `bind_password`

`ldaps://` uses implicit TLS. `ldap://` requires verified StartTLS before any
credentials or directory data are sent.

Group rules map raw LDAP attribute values to canonical group names. Group filters
then evaluate those extracted names; they do not evaluate the complete raw LDAP
attribute value.

### Remote scopes

`remote` scopes define:

- `login_url`
- `timeout_ms`
- optional `default_service_name`
- `jwt_issuer`
- optional `jwt_audience`
- exactly one of:
  - `jwks_url`
  - `jwt_public_key_pem`
  - `jwt_hmac_secret`
- optional `username_claim` defaulting to `sub`
- optional `groups_claim` defaulting to `groups`

The upstream JWT is used only during login. mreg-rust validates it, extracts identity, and then issues its own access token.

## Login API

`POST /api/v2/auth/login` accepts:

```json
{
  "identity_scope": "local",
  "username": "alice",
  "password": "secret",
  "service_name": "mreg",
  "otp_code": "123456"
}
```

`service_name` and `otp_code` are optional. Remote scopes may use them; local and LDAP scopes ignore them.

`GET /api/v2/auth/providers` is unauthenticated and lists safe provider metadata
from the same registry used to route login requests.

`GET /api/v2/auth/me` returns the resolved principal and token expiry for the current request.
