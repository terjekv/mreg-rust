# Event System

mreg-rust emits domain events when resources are created, updated, or deleted. Events are delivered to one or more external sinks (webhooks, AMQP, Redis) for integration with monitoring, automation, or audit pipelines.

## Events vs. Audit vs. Logging

These three systems serve different purposes and should not be confused:

| Concern | Mechanism | Purpose | Audience |
|---------|-----------|---------|----------|
| **Logging** (`tracing`) | Structured log lines to stdout/stderr | Operational diagnostics — request tracing, error details, startup info | Operators, log aggregators (ELK, Datadog) |
| **Audit** (`/api/v1/system/history`) | Immutable records in the database via `AuditStore` | Compliance and accountability — who changed what, when | Security teams, compliance audits, internal forensics |
| **Events** (`EventSink`) | Fire-and-forget delivery to external systems | Real-time integration — trigger workflows, sync caches, notify downstream | External services, automation pipelines |

**Logging** is about what the server is doing. It includes HTTP requests, SQL queries, startup messages, and errors. It is configured via `RUST_LOG` and `MREG_JSON_LOGS`.

**Audit** is about what changed in the domain. Every mutation (create, update, delete) records a `HistoryEvent` in the database with the actor, resource, action, and data diff. Audit records are queryable via the API and persist across restarts.

**Events** are about notifying the outside world. When a mutation succeeds, a `DomainEvent` is emitted to configured sinks. Events are fire-and-forget — if a sink is down, the event is logged as a warning and dropped. For guaranteed delivery, consumers should poll the audit history endpoint.

## Architecture

Events are emitted at the **service layer**, not inside storage backends. This guarantees that every mutation is recorded and emitted regardless of which storage backend is active:

```
API handler
  → service function (audit + event emission happens here)
    → storage trait (persistence only)
```

The service layer:
1. Calls the storage backend to perform the mutation
2. Records an audit event via `AuditStore`
3. Emits a `DomainEvent` to the configured `EventSink`

For **update** operations, the service fetches the current state before the mutation so both old and new values are captured.

## DomainEvent payload

Every event has this structure:

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "actor": "system",
  "resource_kind": "host",
  "resource_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "resource_name": "web.example.org",
  "action": "update",
  "data": {
    "old": { "comment": "old value" },
    "new": { "comment": "new value" }
  },
  "timestamp": "2026-04-03T12:00:00Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Unique event identifier |
| `actor` | string | Who performed the action (currently `"system"`) |
| `resource_kind` | string | Entity type: `host`, `label`, `forward_zone`, `record`, etc. |
| `resource_id` | UUID or null | UUID of the affected resource |
| `resource_name` | string | Human-readable identifier (hostname, zone name, etc.) |
| `action` | string | `create`, `update`, or `delete` |
| `data` | object | Action-specific payload (see below) |
| `timestamp` | ISO 8601 | When the event was recorded |

### Data payload by action

- **create**: Contains the new resource's key fields (e.g., `{"name": "...", "description": "..."}`)
- **update**: Contains `{"old": {...}, "new": {...}}` with the changed fields
- **delete**: Contains the resource's state before deletion

## Configuration

Events are configured via environment variables. If no sink URLs are set, a no-op sink is used and no events are emitted.

| Variable | Default | Description |
|----------|---------|-------------|
| `MREG_EVENT_WEBHOOK_URL` | — | URL to POST events to as JSON |
| `MREG_EVENT_WEBHOOK_TIMEOUT_MS` | `5000` | Timeout for webhook HTTP requests |
| `MREG_EVENT_AMQP_URL` | — | AMQP connection URL (requires `amqp` feature) |
| `MREG_EVENT_AMQP_EXCHANGE` | `mreg.events` | AMQP exchange name (topic type, durable) |
| `MREG_EVENT_REDIS_URL` | — | Redis connection URL (requires `redis` feature) |
| `MREG_EVENT_REDIS_STREAM` | `mreg:events` | Redis Stream key for `XADD` |

Multiple sinks can be active simultaneously. If more than one URL is configured, events are delivered to all of them via a composite sink.

## Sink backends

### Webhook (always available)

POSTs the `DomainEvent` as a JSON body to the configured URL. On failure, retries once after 1 second, then drops the event with a warning log.

```bash
MREG_EVENT_WEBHOOK_URL=https://hooks.example.com/mreg
MREG_EVENT_WEBHOOK_TIMEOUT_MS=3000
```

### AMQP (requires `amqp` feature)

Publishes to a durable topic exchange. The routing key is `{resource_kind}.{action}` (e.g., `host.create`, `forward_zone.delete`), allowing consumers to bind with patterns like `host.*` or `*.delete`.

```bash
MREG_EVENT_AMQP_URL=amqp://guest:guest@localhost:5672
MREG_EVENT_AMQP_EXCHANGE=mreg.events
```

Build with: `cargo build --features amqp`

### Redis Streams (requires `redis` feature)

Appends events to a Redis Stream via `XADD`. Streams provide persistence, consumer groups, and replay — unlike pub/sub which is fire-and-forget.

Each stream entry contains:
- `id`: event UUID
- `resource_kind`, `resource_name`, `action`, `actor`: key fields for filtering
- `payload`: full JSON-serialized `DomainEvent`

```bash
MREG_EVENT_REDIS_URL=redis://localhost:6379
MREG_EVENT_REDIS_STREAM=mreg:events
```

Build with: `cargo build --features redis`

## Error handling

Sink failures are **fire-and-forget**. A failing sink never blocks or rolls back a mutation — the storage operation and audit record have already succeeded. Failures are logged at `warn` level.

For guaranteed delivery, consumers should use the audit history API (`GET /api/v1/system/history`) as the source of truth and treat events as best-effort notifications.

## Feature flags

AMQP and Redis sinks are behind Cargo feature flags to avoid pulling in unnecessary dependencies:

```toml
# Cargo.toml
[features]
amqp = ["dep:lapin"]
redis = ["dep:redis"]
```

The webhook sink and the core event infrastructure are always available.
