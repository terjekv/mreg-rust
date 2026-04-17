# Logging

mreg-rust uses the [tracing](https://docs.rs/tracing) framework for structured, machine-parseable logging with human-readable fallback.

## Logging vs. Audit vs. Events

These three observability systems serve different purposes:

| System | Purpose | Persistence | Audience |
|--------|---------|-------------|----------|
| **Logging** | Operational diagnostics — what the server is doing | Ephemeral (stdout/stderr) | Operators, log aggregators |
| **Audit** | Compliance — who changed what, when | Database (queryable via API) | Security, compliance |
| **Events** | Integration — notify downstream systems | Fire-and-forget to sinks | External services, automation |

Logging captures request flow, errors, and system health. Audit records every domain mutation with before/after state. Events push notifications to webhooks, AMQP, or Redis. See [event-system.md](event-system.md) for the event system.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `MREG_JSON_LOGS` | `false` | Emit logs as JSON (for log aggregators). When `false`, uses human-readable text format. |
| `RUST_LOG` | `info,actix_web=info` | Standard tracing filter directive. Controls which modules log at which level. |

### Examples

```bash
# Human-readable logs (development)
MREG_JSON_LOGS=false cargo run

# JSON logs (production)
MREG_JSON_LOGS=true cargo run

# Debug logging for services
RUST_LOG=info,mreg_rust::services=debug cargo run

# Trace-level logging for everything
RUST_LOG=trace cargo run

# Quiet — errors only
RUST_LOG=error cargo run
```

## Log levels

| Level | Used for | Examples |
|-------|----------|---------|
| **ERROR** | Server errors (5xx), infrastructure failures | Internal errors, Treetop communication failures |
| **WARN** | Client errors (4xx), denied access, degraded operation | Validation errors, not found, authorization denied, event sink failures |
| **INFO** | Startup, request lifecycle, mutations | Server start, request completed, service operations |
| **DEBUG** | Read operations, authorization success, detailed context | List/get operations, authorization allowed |
| **TRACE** | Framework internals (actix, diesel, reqwest) | Not used by application code — framework-level only |

## What is logged

### Startup

When the server starts, a single structured log line records the full configuration:

```
INFO starting mreg-rust
  version="0.1.0"
  git_sha="abc1234"
  address=127.0.0.1:8080
  workers=4
  storage_backend=Postgres
  database_configured=true
  authz_mode="treetop"
  event_sinks="webhook, amqp"
  json_logs=true
  run_migrations=true
```

### HTTP requests

Every request gets a tracing span with these fields:

| Field | Source | Example |
|-------|--------|---------|
| `correlation_id` | `X-Correlation-Id` header, or falls back to `request_id` | `"a1b2c3d4-..."` |
| `request_id` | `X-Request-Id` header or auto-generated UUID | `"550e8400-e29b-41d4-..."` |
| `principal` | `X-Mreg-User` header or `"anonymous"` | `"admin"` |
| `http.method` | Request method | `"POST"` |
| `http.target` | Request path | `"/api/v1/inventory/hosts"` |
| `http.status_code` | Response status (set on completion) | `201` |

A log line is emitted when the request completes:
- 2xx → `INFO request completed`
- 4xx → `WARN client error`
- 5xx → `ERROR request failed`

Both `X-Request-Id` and `X-Correlation-Id` are returned in the response headers.

### Correlation ID vs. Request ID

- **`correlation_id`** traces a logical operation across multiple services. It is propagated from upstream callers via the `X-Correlation-Id` header. When not provided, the field is empty — only requests that originate from a system that sets correlation IDs will have one.
- **`request_id`** is unique to this server instance — each request gets its own, either from `X-Request-Id` or auto-generated.

The `correlation_id` is the **outermost field** in the span, so every log line within a request includes it. This makes it easy to follow a single user action across mreg-rust and any upstream/downstream services that propagate the same correlation ID.

### Errors

All `AppError` responses are logged with the error kind and message. These log lines inherit the request span, so they automatically include `request_id`, `principal`, `http.method`, and `http.target`.

| Status range | Level | Example |
|-------------|-------|---------|
| 400 (Validation) | WARN | `client error error_kind="validation_error" status=400 error="validation error: label name cannot be empty"` |
| 404 (Not found) | WARN | `client error error_kind="not_found" status=404 error="not found: label 'missing'"` |
| 409 (Conflict) | WARN | `client error error_kind="conflict" status=409` |
| 403 (Forbidden) | WARN | `client error error_kind="forbidden" status=403` |
| 500 (Internal) | ERROR | `server error error_kind="internal_error" status=500` |
| 502 (Authz) | ERROR | `server error error_kind="authz_error" status=502` |
| 503 (Unavailable) | ERROR | `server error error_kind="service_unavailable" status=503` |

### Service operations

All service-layer functions have tracing spans with `resource_kind` fields. Mutation functions (create, update, delete) are logged at INFO level; read functions (list, get) at DEBUG level.

Fields available in service spans:
- `resource_kind` — entity type (`"host"`, `"label"`, `"forward_zone"`, etc.)
- Command/name parameters — logged via Debug formatting

### Authorization

| Event | Level | Fields |
|-------|-------|--------|
| Permission granted | DEBUG | `principal`, `action`, `resource_kind`, `resource_id` |
| Permission denied | WARN | `principal`, `action`, `resource_kind`, `resource_id` |
| Treetop request failed | ERROR | `treetop_url`, `error` |
| Treetop unexpected status | ERROR | `treetop_url`, `status` |

### Event sink failures

| Event | Level | Fields |
|-------|-------|--------|
| Webhook delivery failed | WARN | `url`, `resource_kind`, `action`, `error` |
| Webhook retry failed | WARN | `url`, `resource_kind`, `action`, `error` |
| AMQP publish failed | WARN | `exchange`, `routing_key`, `error` |
| Redis XADD failed | WARN | `stream`, `error` |

## JSON log format

When `MREG_JSON_LOGS=true`, each log line is a self-contained JSON object:

```json
{
  "timestamp": "2026-04-03T12:00:00.123456Z",
  "level": "INFO",
  "target": "mreg_rust::middleware::root_span",
  "span": {
    "correlation_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "request_id": "550e8400-e29b-41d4-a716-446655440000",
    "principal": "admin",
    "http.method": "POST",
    "http.target": "/api/v1/inventory/hosts",
    "http.status_code": 201
  },
  "spans": [
    {
      "name": "http_request",
      "correlation_id": "a1b2c3d4-...",
      "request_id": "550e8400-...",
      "principal": "admin"
    },
    {
      "name": "create",
      "resource_kind": "host"
    }
  ],
  "fields": {
    "message": "request completed"
  }
}
```

### Filtering JSON logs

Common queries for JSON log processing:

```bash
# All errors
jq 'select(.level == "ERROR")' < logs.jsonl

# All requests by a specific user
jq 'select(.span.principal == "admin")' < logs.jsonl

# All mutations (service-level spans)
jq 'select(.spans[]?.resource_kind != null)' < logs.jsonl

# Correlate by request ID
jq 'select(.span.request_id == "550e8400-...")' < logs.jsonl

# Trace across services by correlation ID
jq 'select(.span.correlation_id == "a1b2c3d4-...")' < logs.jsonl

# All authorization denials
jq 'select(.fields.message == "authorization denied")' < logs.jsonl
```

## Structured fields reference

All field names used in the codebase:

| Field | Type | Where | Description |
|-------|------|-------|-------------|
| `correlation_id` | string (UUID) | Request span | Cross-service trace ID from `X-Correlation-Id` (empty if not provided) |
| `request_id` | string (UUID) | Request span | Unique per-request ID for this server |
| `principal` | string | Request span | Authenticated user from `X-Mreg-User` |
| `http.method` | string | Request span | HTTP method |
| `http.target` | string | Request span | Request path |
| `http.status_code` | integer | Request span | Response status code |
| `resource_kind` | string | Service span | Entity type being operated on |
| `error_kind` | string | Error log | Error variant name |
| `error` | string | Error/warn logs | Error message |
| `status` | integer | Error log | HTTP status code |
| `action` | string | Auth/event logs | Operation being performed |
| `url` | string | Webhook/Treetop logs | Target URL |
| `exchange` | string | AMQP logs | AMQP exchange name |
| `routing_key` | string | AMQP logs | AMQP routing key |
| `stream` | string | Redis logs | Redis Stream key |
| `version` | string | Startup | Application version |
| `storage_backend` | string | Startup | Active storage backend |
| `authz_mode` | string | Startup | Authorization mode |
| `event_sinks` | string | Startup | Active event sinks |
