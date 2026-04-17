# Authorization Action Matrix

This document defines the target authorization shape for `mreg-rust` when wiring requests to
`treetop-rest`.

The goal is:

- granular, stable actions on the `mreg-rust` side
- minimal, resource-local attrs by default
- action-specific attr expansion only when policy genuinely needs it
- no "send the full object graph" behavior

## Rules

### Action naming

- Use dotted action IDs such as `host.get`, `host.update.zone`, `host.ip.assign_auto`.
- Split `list` and `get`.
- Split partial updates by changed field or field-group when permissions may differ.
- Split relationship mutations into `attach` and `detach`.
- Keep derived side effects under the initiating action.
  - Example: `host.ip.assign_auto` covers the derived A/AAAA/PTR writes created by the storage layer.

### Attr policy

For each check, send:

- the resource `kind`
- the resource `id`
- the resource's `core attrs`
- only the additional `action attrs` needed for that action

Do not send:

- unrelated resource families
- full transitive graphs
- "all known attrs just in case"

## Lookup Cost Legend

- `none`: available from path, query, or payload
- `light`: one direct lookup of the target resource
- `medium`: one direct lookup plus one related lookup
- `heavy`: multiple related lookups or graph expansion

The implementation target should keep most checks at `none` or `light`.

## Core Attr Sets

These are the default attrs for each resource kind. Action-specific attrs should extend these
instead of replacing them.

| Resource kind | Resource ID | Core attrs |
|---|---|---|
| `host` | hostname | `name`, `zone` |
| `ip_address` | IP address | `address`, `family`, `host_name`, `network` |
| `label` | label name | `name` |
| `nameserver` | nameserver FQDN | `name` |
| `forward_zone` | zone name | `name`, `primary_ns` |
| `reverse_zone` | zone name | `name`, `network`, `primary_ns` |
| `forward_zone_delegation` | delegation UUID | `zone_name`, `delegation_name` |
| `reverse_zone_delegation` | delegation UUID | `zone_name`, `delegation_name` |
| `network` | CIDR | `cidr`, `category`, `location`, `dns_delegated`, `frozen` |
| `excluded_range` | `network:start-end` | `network`, `start_ip`, `end_ip` |
| `record_type` | type name | `name`, `dns_type`, `owner_kind`, `built_in` |
| `record` | record UUID | `type_name`, `owner_kind`, `owner_name` |
| `rrset` | rrset UUID | `type_name`, `owner_name`, `anchor_kind`, `anchor_name` |
| `host_contact` | email | `email` |
| `host_group` | group name | `name` |
| `bacnet_id` | BACnet ID | `bacnet_id`, `host_name` |
| `ptr_override` | IP address | `address`, `host_name`, `target_name` |
| `network_policy` | policy name | `name`, `community_template_pattern` |
| `community` | community UUID | `policy_name`, `network`, `name` |
| `attachment_community_assignment` | assignment UUID | `attachment_id`, `network`, `policy_name`, `community_name` |
| `auth_session` | principal ID | none |
| `host_policy_atom` | atom name | `name` |
| `host_policy_role` | role name | `name` |
| `import_batch` | batch UUID | `requested_by` |
| `export_template` | template name | `name`, `engine`, `scope` |
| `export_run` | run UUID | `template_name`, `scope`, `requested_by` |
| `task` | task UUID | `kind`, `state` |

## Action Matrix

### System and Audit

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `system.health.get` | `system` | none | `none` | May remain unauthenticated if desired. |
| `system.version.get` | `system` | none | `none` | May remain unauthenticated if desired. |
| `system.status.get` | `system` | none | `none` | May remain unauthenticated if desired. |
| `audit.history.list` | `audit_history` | none | `none` | If filtered later, include only the filter terms. |

### Hosts and IP Addresses

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `host.list` | `host` | query filter summary only | `none` | Avoid expanding every host just for auth. |
| `host.get` | `host` | `labels`, `host_groups`, `addresses`, `networks` | `light` | `networks` should always be present as a set, even when empty. Network-scoped policy should use `resource.networks.contains(...)`. |
| `host.create` | `host` | `labels=[]`, `host_groups=[]`, `addresses=[]`, `networks=[]` | `none` | Payload already contains `name`, `zone`, `ttl`, `comment`. |
| `host.update.name` | `host` | `new_name`, `networks` | `light` | Trigger only if `name` changes. |
| `host.update.zone` | `host` | `new_zone`, `networks` | `light` | Trigger only if `zone` changes. |
| `host.update.ttl` | `host` | `new_ttl`, `networks` | `light` | Trigger only if `ttl` changes. |
| `host.update.comment` | `host` | `networks` | `light` | Preserves legacy network-scoped host policies without changing the action model. |
| `host.delete` | `host` | `labels`, `host_groups`, `addresses`, `networks` | `light` | Useful if delete policy depends on attached state. |
| `host.ip.list` | `ip_address` | none | `none` | Global list of assignments. |
| `host.ip.list_for_host` | `host` | `addresses`, `networks` | `light` | Authorize on host, not each IP row. |
| `host.ip.assign_manual` | `ip_address` | `host_name`, `network`, `allocation_mode="manual"` | `none` | Prefer authorizing this on `ip_address`; optionally also check `host`. |
| `host.ip.assign_auto` | `ip_address` | `host_name`, `network`, `allocation_mode="auto"` | `none` | Covers auto-created DNS side effects. |
| `host.ip.update.mac` | `ip_address` | none | `light` | No need for host labels unless policy explicitly wants them. |
| `host.ip.unassign` | `ip_address` | `host_name`, `network` | `light` | Covers cleanup of A/AAAA/PTR side effects. |

### Labels

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `label.list` | `label` | none | `none` | |
| `label.get` | `label` | none | `light` | |
| `label.create` | `label` | none | `none` | |
| `label.update.description` | `label` | none | `light` | |
| `label.delete` | `label` | none | `light` | |

### Nameservers

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `nameserver.list` | `nameserver` | none | `none` | |
| `nameserver.get` | `nameserver` | `ttl` | `light` | |
| `nameserver.create` | `nameserver` | `ttl` | `none` | |
| `nameserver.update.ttl` | `nameserver` | `new_ttl` | `light` | |
| `nameserver.delete` | `nameserver` | none | `light` | |

### Zones and Delegations

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `zone.forward.list` | `forward_zone` | none | `none` | |
| `zone.forward.get` | `forward_zone` | `nameservers`, `email` | `light` | |
| `zone.forward.create` | `forward_zone` | `nameservers`, `email`, `serial_no`, `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl` | `none` | |
| `zone.forward.update.primary_ns` | `forward_zone` | `new_primary_ns` | `light` | |
| `zone.forward.update.nameservers` | `forward_zone` | `new_nameservers` | `light` | |
| `zone.forward.update.email` | `forward_zone` | `new_email` | `light` | |
| `zone.forward.update.timing` | `forward_zone` | changed subset of `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl` | `light` | Group timing/TTL knobs unless you need separate policy. |
| `zone.forward.delete` | `forward_zone` | none | `light` | |
| `zone.reverse.list` | `reverse_zone` | none | `none` | |
| `zone.reverse.get` | `reverse_zone` | `nameservers`, `email` | `light` | |
| `zone.reverse.create` | `reverse_zone` | `nameservers`, `email`, `serial_no`, `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl` | `none` | |
| `zone.reverse.update.primary_ns` | `reverse_zone` | `new_primary_ns` | `light` | |
| `zone.reverse.update.nameservers` | `reverse_zone` | `new_nameservers` | `light` | |
| `zone.reverse.update.email` | `reverse_zone` | `new_email` | `light` | |
| `zone.reverse.update.timing` | `reverse_zone` | changed subset of `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl` | `light` | |
| `zone.reverse.delete` | `reverse_zone` | none | `light` | |
| `zone.forward.delegation.list` | `forward_zone` | none | `none` | Authorize list on parent zone. |
| `zone.forward.delegation.create` | `forward_zone_delegation` | `comment`, `nameservers`, `zone_name` | `none` | |
| `zone.forward.delegation.delete` | `forward_zone_delegation` | none | `light` | |
| `zone.reverse.delegation.list` | `reverse_zone` | none | `none` | Authorize list on parent zone. |
| `zone.reverse.delegation.create` | `reverse_zone_delegation` | `comment`, `nameservers`, `zone_name` | `none` | |
| `zone.reverse.delegation.delete` | `reverse_zone_delegation` | none | `light` | |

### Networks

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `network.list` | `network` | query filter summary only | `none` | |
| `network.get` | `network` | `vlan`, `reserved` | `light` | |
| `network.create` | `network` | `vlan`, `reserved` | `none` | |
| `network.update.description` | `network` | none | `light` | |
| `network.update.vlan` | `network` | `new_vlan` | `light` | |
| `network.update.dns_delegated` | `network` | `new_dns_delegated` | `light` | |
| `network.update.category` | `network` | `new_category` | `light` | |
| `network.update.location` | `network` | `new_location` | `light` | |
| `network.update.frozen` | `network` | `new_frozen` | `light` | |
| `network.update.reserved` | `network` | `new_reserved` | `light` | |
| `network.delete` | `network` | none | `light` | |
| `network.excluded_range.list` | `network` | none | `none` | Authorize on parent network. |
| `network.excluded_range.create` | `excluded_range` | `description` | `none` | |
| `network.address.list_used` | `network` | none | `none` | |
| `network.address.list_unused` | `network` | `limit` | `none` | |

### Records and Record Types

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `record_type.list` | `record_type` | none | `none` | |
| `record_type.create` | `record_type` | `fields`, `cardinality`, `zone_bound`, `render_template` | `none` | |
| `record_type.delete` | `record_type` | `built_in` | `light` | |
| `record.list` | `record` | query filter summary only | `none` | |
| `record.get` | `record` | `ttl`, `anchor_name`, `zone_id` | `light` | |
| `rrset.list` | `rrset` | none | `none` | |
| `rrset.get` | `rrset` | `ttl`, `zone_id` | `light` | |
| `record.create.anchored` | `record` | `ttl`, `anchor_name`, `zone_id`, `is_unanchored=false` | `none` | |
| `record.create.unanchored` | `record` | `ttl`, `zone_id`, `is_unanchored=true` | `none` | |
| `record.update.ttl` | `record` | `new_ttl` | `light` | Trigger only if TTL changes. |
| `record.update.data` | `record` | `new_type_name`, `new_owner_name` only if changed | `light` | Most updates only need record-local facts. |
| `record.delete` | `record` | none | `light` | |
| `rrset.delete` | `rrset` | none | `light` | |

### Host Contacts, Groups, BACnet, PTR Overrides

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `host_contact.list` | `host_contact` | query filter summary only | `none` | |
| `host_contact.get` | `host_contact` | `display_name`, `hosts` | `light` | |
| `host_contact.create` | `host_contact` | `display_name`, `hosts` | `none` | |
| `host_contact.delete` | `host_contact` | none | `light` | |
| `host_group.list` | `host_group` | query filter summary only | `none` | |
| `host_group.get` | `host_group` | `hosts`, `parent_groups`, `owner_groups` | `light` | |
| `host_group.create` | `host_group` | `hosts`, `parent_groups`, `owner_groups`, `description` | `none` | |
| `host_group.delete` | `host_group` | none | `light` | |
| `bacnet_id.list` | `bacnet_id` | query filter summary only | `none` | |
| `bacnet_id.get` | `bacnet_id` | none | `light` | |
| `bacnet_id.create` | `bacnet_id` | none | `none` | |
| `bacnet_id.delete` | `bacnet_id` | none | `light` | |
| `ptr_override.list` | `ptr_override` | query filter summary only | `none` | |
| `ptr_override.get` | `ptr_override` | none | `light` | |
| `ptr_override.create` | `ptr_override` | none | `none` | |
| `ptr_override.delete` | `ptr_override` | none | `light` | |

### Network Policies, Communities, and Attachment-Community Assignments

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `network_policy.list` | `network_policy` | query filter summary only | `none` | |
| `network_policy.get` | `network_policy` | none | `light` | |
| `network_policy.create` | `network_policy` | `description`, `community_template_pattern` | `none` | |
| `network_policy.delete` | `network_policy` | none | `light` | |
| `community.list` | `community` | query filter summary only | `none` | |
| `community.get` | `community` | `description` | `light` | |
| `community.create` | `community` | `description` | `none` | |
| `community.delete` | `community` | none | `light` | |
| `attachment_community_assignment.list` | `attachment_community_assignment` | query filter summary only | `none` | |
| `attachment_community_assignment.get` | `attachment_community_assignment` | none | `light` | |
| `attachment_community_assignment.create` | `attachment_community_assignment` | none | `none` | |
| `attachment_community_assignment.delete` | `attachment_community_assignment` | none | `light` | |

### Host Policy

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `host_policy.atom.list` | `host_policy_atom` | none | `none` | |
| `host_policy.atom.get` | `host_policy_atom` | `description` | `light` | |
| `host_policy.atom.create` | `host_policy_atom` | `description` | `none` | |
| `host_policy.atom.update.description` | `host_policy_atom` | none | `light` | |
| `host_policy.atom.delete` | `host_policy_atom` | none | `light` | |
| `host_policy.role.list` | `host_policy_role` | none | `none` | |
| `host_policy.role.get` | `host_policy_role` | `atoms`, `hosts`, `labels` | `light` | |
| `host_policy.role.create` | `host_policy_role` | `description` | `none` | |
| `host_policy.role.update.description` | `host_policy_role` | none | `light` | |
| `host_policy.role.delete` | `host_policy_role` | none | `light` | |
| `host_policy.role.atom.attach` | `host_policy_role` | `atom` | `none` | Target role is the primary authz resource. |
| `host_policy.role.atom.detach` | `host_policy_role` | `atom` | `none` | |
| `host_policy.role.host.attach` | `host_policy_role` | `host` | `none` | |
| `host_policy.role.host.detach` | `host_policy_role` | `host` | `none` | |
| `host_policy.role.label.attach` | `host_policy_role` | `label` | `none` | |
| `host_policy.role.label.detach` | `host_policy_role` | `label` | `none` | |

### Authentication Session Administration

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `auth_session.logout_all` | `auth_session` | none | `none` | Revokes all sessions for the target principal. Intended for admin or other explicitly authorized operators. |

### Imports, Exports, Tasks, Workers

| Action | Resource kind | Action attrs | Lookup cost | Notes |
|---|---|---|---|---|
| `task.list` | `task` | none | `none` | Applies to `/tasks`. |
| `import.batch.list` | `import_batch` | none | `none` | Applies to `/imports` GET. |
| `import.batch.create` | `import_batch` | `requested_by`, `item_count`, `item_kinds`, `item_operations` | `none` | Do not send full item payloads by default. |
| `import.batch.run` | `import_batch` | none | `light` | Applies when executing an existing batch. |
| `export.template.list` | `export_template` | none | `none` | Applies to `/export-templates` GET. |
| `export.template.create` | `export_template` | `engine`, `scope` | `none` | Do not send template body unless policy truly depends on it. |
| `export.run.list` | `export_run` | none | `none` | Applies to `/export-runs` GET. |
| `export.run.create` | `export_run` | `template_name`, `scope`, `requested_by` | `none` | |
| `export.run.execute` | `export_run` | none | `light` | Use when an existing run is executed. |
| `worker.task.claim_next` | `task` | none | `none` | Intended for service-account / worker policy only. |
| `worker.task.execute.import_batch` | `task` | `kind="import_batch"` | `light` | |
| `worker.task.execute.export_run` | `task` | `kind="export_run"` | `light` | |

## Implementation Notes

### PATCH handling

PATCH endpoints should authorize by changed field-group, not by generic `update_*`.

Recommended mapping:

- `hosts/{name}`
  - `name` -> `host.update.name`
  - `zone` -> `host.update.zone`
  - `ttl` -> `host.update.ttl`
  - `comment` -> `host.update.comment`

- `nameservers/{name}`
  - `ttl` -> `nameserver.update.ttl`

- `forward-zones/{name}` and `reverse-zones/{name}`
  - `primary_ns` -> `zone.*.update.primary_ns`
  - `nameservers` -> `zone.*.update.nameservers`
  - `email` -> `zone.*.update.email`
  - timing and TTL knobs -> `zone.*.update.timing`

- `networks/{cidr}`
  - one action per changed field listed in the matrix above

- `records/{id}`
  - TTL change -> `record.update.ttl`
  - data or raw RDATA change -> `record.update.data`

### Multi-check requests

If one request changes multiple protected aspects:

- compute all required actions
- deduplicate them
- send a Treetop batch authorize request
- reject if any required action is denied

### Scope discipline

When in doubt:

- prefer authorizing on the directly targeted resource
- add one-hop related attrs before adding more lookups
- avoid heavy graph assembly unless a concrete policy requires it
