# Import Format

This page defines the contract for bulk imports via `POST /api/v1/workflows/imports`.
It covers payload shape, execution flow, reference rules, backend compatibility,
failure behavior, and migration-focused examples.

## Shape

Imports are staged as a batch document:

```json
{
  "items": [
    {
      "ref": "host-1",
      "kind": "host",
      "operation": "create",
      "attributes": {
        "name": "example.uio.no"
      }
    }
  ]
}
```

Optional top-level fields:

- `requested_by`: free-form identity string used for task/audit context.

## Rules

- `ref` is unique within the batch and used for cross-item references.
- `kind` maps to a domain entity family.
- MVP currently supports `create`.
- `attributes` contains the entity payload.
- references to other staged items use their `ref` instead of database IDs.

Validation baseline:

- batch must contain at least one item.
- item `ref` cannot be empty.
- unknown `kind` values are rejected.
- unknown `operation` values are rejected.

## Execution Model

Imports are a staged workflow, not a single synchronous mutation:

1. `POST /api/v1/workflows/imports` stores the batch and creates an `import_batch` task.
2. A worker (or automation) calls `POST /api/v1/workflows/tasks/run-next` to claim and execute queued tasks (including imports).
3. `GET /api/v1/workflows/imports` shows status and summaries (`validation_report`, `commit_summary`).

This means `POST /workflows/imports` by itself does not apply domain entities yet.
In production, a worker should continuously run the task runner flow; manual
`run-next` calls are mainly for tests, debugging, or ad-hoc operations.

See [task-system.md](task-system.md) for task lifecycle details and worker
execution semantics.

## Status Lifecycle

Import batches use these status values:

- `queued`
- `validating`
- `ready`
- `committing`
- `succeeded`
- `failed`
- `cancelled`

Current import execution path transitions through `queued` -> `validating` -> `succeeded`
or `failed`.

Summary fields exposed in import listing:

- `validation_report`: validation/diagnostic metadata.
- `commit_summary`: populated on success with applied item results and count.

## Current Kinds

The PostgreSQL import executor currently supports these `kind` values:

- `label`
- `nameserver`
- `network`
- `excluded_range`
- `forward_zone`
- `reverse_zone`
- `forward_zone_delegation`
- `reverse_zone_delegation`
- `host`
- `host_attachment`
- `ip_address`
- `record`
- `attachment_dhcp_identifier`
- `attachment_prefix_reservation`
- `attachment_community_assignment`
- `host_contact`
- `host_group`
- `bacnet_id`
- `ptr_override`
- `network_policy`
- `network_policy_attribute`
- `network_policy_attribute_value`
- `community`
- `host_community_assignment`
- `host_policy_atom`
- `host_policy_role`
- `host_policy_role_atom`
- `host_policy_role_host`
- `host_policy_role_label`

The memory backend supports this subset:

- `label`
- `nameserver`
- `network`
- `host_contact`
- `host_group`
- `bacnet_id`
- `ptr_override`
- `network_policy`
- `community`
- `forward_zone`
- `reverse_zone`
- `excluded_range`
- `host`
- `host_attachment`
- `ip_address`
- `record`
- `attachment_dhcp_identifier`
- `attachment_prefix_reservation`
- `attachment_community_assignment`
- `host_community_assignment`

The memory backend currently does not support delegation imports, network policy
attribute/value imports, or host policy imports. Unsupported kinds fail with a
validation error.

For legacy Django migration batches, PostgreSQL is the target backend.

The importer keeps the HTTP payload string-oriented for compatibility, but each item is parsed into typed domain commands before anything is committed.

## Cross-Item References

Scalar references can be supplied with `<field>_ref`, for example:

```json
{
  "ref": "host-1",
  "kind": "host",
  "operation": "create",
  "attributes": {
    "name": "app.example.org",
    "zone_ref": "zone-1"
  }
}
```

Array-valued fields such as `nameservers` can also refer to earlier values by
using the referenced batch value directly in the array.

### Ordering Requirement

References are resolved while items are processed in batch order. A referenced
`ref` must therefore appear earlier in the same `items` list.

In practice:

- producer order matters.
- place foundational entities first (nameservers, policies, zones, hosts,
  attachments).
- place dependent entities later (records, assignment links, policy mappings).

Some import kinds accept migration-friendly aliases for natural keys:

- `attachment_community_assignment`: `attachment_id` or `attachment_id_ref`
- `attachment_dhcp_identifier`: `attachment_id` or `attachment_id_ref`
- `attachment_prefix_reservation`: `attachment_id` or `attachment_id_ref`
- `community`: `network` or `network_cidr`
- `forward_zone_delegation`: `zone` or `zone_name`
- `reverse_zone_delegation`: `zone` or `zone_name`
- `network`: `policy` or `policy_name`

`ip_address` now supports attachment-first imports. Preferred shape:

```json
{
  "ref": "ip-1",
  "kind": "ip_address",
  "operation": "create",
  "attributes": {
    "attachment_id_ref": "attachment-1",
    "address": "192.0.2.20"
  }
}
```

Legacy `host_name` plus `network`/`mac_address` input is still accepted, but new
migration tooling should emit explicit `host_attachment` items first and then
reference them from `ip_address`, `attachment_dhcp_identifier`,
`attachment_prefix_reservation`, and `attachment_community_assignment`.

## Validation and Failure Semantics

- Any failing item fails the whole batch.
- Domain entities are not partially persisted from a failed batch.
- Failed item diagnostics are surfaced with item context (`ref` and `kind`) in
  error messages and summaries.
- On successful commit, `commit_summary` includes an `applied` array and total
  `count`.

## Processing Stages

1. Persist raw batch.
2. Normalize input into typed items.
3. Resolve internal references.
4. Validate the full graph.
5. Commit all changes in one database transaction.
6. Persist validation and commit summaries for task/history visibility.

## Atomicity

- a batch either commits fully or not at all.
- no partial persistence of domain entities is allowed when validation fails.
- task logs and validation artifacts may persist independently of the commit transaction.

## Examples

### Minimal staged batch

```json
POST /api/v1/workflows/imports
{
  "requested_by": "migration-script",
  "items": [
    {
      "ref": "net-1",
      "kind": "network",
      "operation": "create",
      "attributes": {
        "cidr": "192.0.2.0/24",
        "description": "Imported network"
      }
    }
  ]
}
```

Typical create response (staged, not yet executed):

```json
{
  "id": "a15dbb9f-4ab2-43b4-a3e1-16e0bb7a2d28",
  "task_id": "3a931c34-43d2-4903-a364-78540d524f77",
  "status": "queued",
  "requested_by": "migration-script",
  "validation_report": null,
  "commit_summary": null
}
```

Then execute pending tasks:

```json
POST /api/v1/workflows/tasks/run-next
{}
```

### Attachment-first IP import

```json
POST /api/v1/workflows/imports
{
  "items": [
    {
      "ref": "ns-1",
      "kind": "nameserver",
      "operation": "create",
      "attributes": {
        "name": "ns1.example.org"
      }
    },
    {
      "ref": "network-1",
      "kind": "network",
      "operation": "create",
      "attributes": {
        "cidr": "198.51.100.0/24",
        "description": "App network"
      }
    },
    {
      "ref": "zone-1",
      "kind": "forward_zone",
      "operation": "create",
      "attributes": {
        "name": "example.org",
        "primary_ns": "ns1.example.org",
        "nameservers": ["ns1.example.org"],
        "email": "hostmaster@example.org"
      }
    },
    {
      "ref": "host-1",
      "kind": "host",
      "operation": "create",
      "attributes": {
        "name": "app.example.org",
        "zone_ref": "zone-1"
      }
    },
    {
      "ref": "attachment-1",
      "kind": "host_attachment",
      "operation": "create",
      "attributes": {
        "host_name_ref": "host-1",
        "network_ref": "network-1",
        "mac_address": "aa:bb:cc:dd:ee:ff"
      }
    },
    {
      "ref": "ip-1",
      "kind": "ip_address",
      "operation": "create",
      "attributes": {
        "attachment_id_ref": "attachment-1",
        "address": "198.51.100.20"
      }
    }
  ]
}
```

### Atomic rollback on late failure

```json
POST /api/v1/workflows/imports
{
  "items": [
    {
      "ref": "network-1",
      "kind": "network",
      "operation": "create",
      "attributes": {
        "cidr": "203.0.113.0/24",
        "description": "Should roll back"
      }
    },
    {
      "ref": "bad-host",
      "kind": "host",
      "operation": "create",
      "attributes": {
        "name": "bad.missing-zone.example",
        "zone": "missing-zone.example"
      }
    }
  ]
}
```

After task execution, the batch status is `failed` and the previously valid
`network` item is rolled back with the rest of the batch.

## Troubleshooting

- `import batch cannot be empty`:
  send at least one item in `items`.
- `import item ref is required`:
  ensure every item has a non-empty `ref`.
- `unknown variant ... kind/operation`:
  correct `kind` or `operation` to supported values.
- `missing required import attribute ...`:
  add required `attributes` fields for that kind.
- `... was not found`:
  reorder items so referenced dependencies are created earlier, or provide
  valid natural keys.
- `... already exists`:
  remove duplicates, adjust natural keys, or split import scope.
- `unsupported import kind ... for memory backend`:
  run migration imports against PostgreSQL backend.
