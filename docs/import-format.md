# Import Format

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

## Rules

- `ref` is unique within the batch and used for cross-item references.
- `kind` maps to a domain entity family.
- MVP currently supports `create`.
- `attributes` contains the entity payload.
- references to other staged items use their `ref` instead of database IDs.

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

The memory backend supports a compatible subset focused on structural inventory imports, but
the migration target for legacy Django exports is PostgreSQL.

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

Array-valued fields such as `nameservers` can also refer to earlier values by using the referenced batch value directly in the array.

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
