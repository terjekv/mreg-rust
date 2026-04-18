# Task System

mreg-rust uses a persisted task queue for asynchronous workflows such as imports and export runs.

## Purpose

Tasks decouple request-time validation from background execution.

- API endpoints create queued tasks with workflow-specific payloads.
- Workers claim the next available task and execute it.
- Task status and results are persisted and queryable.

## Data Model

A task envelope includes:

- `id`: UUID
- `kind`: task type string (for example `import_batch`, `export_run`)
- `status`: `queued`, `running`, `succeeded`, `failed`, `cancelled`
- `payload`: workflow input (for example `{ "import_id": "..." }`)
- `progress`: JSON status metadata
- `result`: optional workflow result payload
- `error_summary`: optional failure summary
- `attempts`, `max_attempts`
- `available_at`, `started_at`, `finished_at`

## Core Endpoints

Workflows API:

- `GET /api/v1/workflows/tasks` lists tasks.
- `POST /api/v1/workflows/tasks/run-next` claims and executes one queued task.

Workflow creators:

- `POST /api/v1/workflows/imports` creates an `import_batch` task.
- `POST /api/v1/workflows/export-runs` creates an `export_run` task.

## run-next Semantics

`POST /api/v1/workflows/tasks/run-next` is a worker-facing endpoint.

Behavior:

1. Claims the next available queued task.
2. Dispatches by `kind`.
3. Marks task succeeded with `result` on success.
4. Marks task failed with `error_summary` on failure.
5. Returns a no-op payload when no task is available.

Current built-in dispatch kinds:

- `import_batch`
- `export_run`

Unknown kinds are completed as no-op with `{ "status": "noop" }`.

## Lifecycle

Typical lifecycle:

1. `queued` after creation.
2. `running` after claim.
3. `succeeded` or `failed` after execution.

`cancelled` is part of the domain status model for parity with other workflows.

## Backend Notes

PostgreSQL and memory backends both implement the same `TaskStore` interface.

- PostgreSQL claiming uses `FOR UPDATE SKIP LOCKED` for safe concurrent workers.
- Memory backend claims in deterministic order from in-process state.

## Worker Model

The API server stores and serves task state, but workflow execution still requires a worker loop that calls `run-next`.

In practice, production should run one or more automation workers that repeatedly invoke:

```bash
curl -X POST http://localhost:8080/api/v1/workflows/tasks/run-next
```

Manual `run-next` calls are useful for tests, debugging, and ad-hoc operations.

## Authorization

Task execution uses worker-scoped auth actions:

- `worker.task.claim_next`
- `worker.task.execute.import_batch`
- `worker.task.execute.export_run`

See `docs/authz-action-matrix.md` for the complete action matrix and attributes.

## Related Docs

- `docs/import-format.md` for import batch contract and status views.
- `docs/export-templating.md` for export run behavior and rendering lifecycle.
- `docs/architecture.md` for high-level worker/task architecture.
