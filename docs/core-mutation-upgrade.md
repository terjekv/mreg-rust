# Core mutation upgrade

The native API stays at `/api/v1` in this prerequisite change. The Django compatibility API and API version move are separate changes.

## Before upgrading

Back up the database. Run migration `00000000000004_network_policy_attribute_order` before `00000000000005_community_policy_invariants`. The first adds stable policy attribute ordering and requires optional community template identifiers to be unique. Resolve duplicates first.

The second migration deliberately aborts when a community's policy differs from its network's policy, or a network's maximum is negative or below its current community count. Inspect affected networks with:

```sql
SELECT n.network, n.policy_id AS network_policy, c.policy_id AS community_policy, c.name
FROM networks n JOIN communities c ON c.network_id = n.id
WHERE n.policy_id IS DISTINCT FROM c.policy_id;

SELECT n.network, n.max_communities, count(c.id)
FROM networks n LEFT JOIN communities c ON c.network_id = n.id
GROUP BY n.id
HAVING n.max_communities < 0 OR n.max_communities < count(c.id);
```

Explicitly assign each network its intended policy and reconcile communities using other policies. Existing installations without an API for policy assignment must perform the repair through an administrator-controlled database migration. Unfreeze affected networks before repairing their graph and restore their frozen state afterward. Raise limits to cover existing communities, or remove unwanted communities explicitly. The migration does not select policies, delete inventory or loosen limits automatically.

Policy and community descriptions must be nonblank. Policy names must be at most 100 characters. Optional community template identifiers must contain 1–100 ASCII letters, digits or underscores; normalize surrounding whitespace and resolve duplicates before migrating. Invalid persisted values must be repaired before the new server loads them.

Policy-attribute names must contain 1–100 ASCII lowercase letters, digits, dots, hyphens or underscores. Imports now trim and lowercase attribute definitions and membership references, matching API writes. Older PostgreSQL imports could store unchecked names. Inspect existing definitions before upgrading:

```sql
SELECT id, name FROM network_policy_attributes
WHERE name !~ '^[a-z0-9_.-]{1,100}$';

SELECT lower(btrim(name)) AS normalized_name, count(*)
FROM network_policy_attributes
GROUP BY lower(btrim(name))
HAVING count(*) > 1;
```

Repair invalid names and resolve normalization collisions explicitly, preserving the IDs referenced by policy memberships. Attribute listing and name-based API operations require canonical stored names.

## Behavior

All community creation paths enforce the same policy and capacity checks, including bulk imports and direct storage callers. Imports roll back on violations. Network updates cannot clear or replace a policy still used by communities or lower the limit below the current count. PostgreSQL also protects direct SQL writes with constraints and triggers; creators serialize on the network row.

Clearing or deleting a policy also clears its networks' community limits. Migration `00000000000005_community_policy_invariants` handles PostgreSQL foreign-key cascades atomically. A subsequent policy assignment starts without a limit unless one is explicitly supplied.

Policy PATCH requests preserve omitted fields during concurrent writes. The native attribute list supports the standard `limit`, `after`, `sort_by` and `sort_dir` parameters; it defaults to 100 entries and caps public pages at 1000.

Delegation updates preserve DS and glue records and existing unchanged NS identities. Comment-only updates do not change DNS records or the parent serial. An explicitly empty nameserver list is invalid.

IP moves validate the requested attachment and retain the assignment identity. Remove a PTR override explicitly before moving its address; moves with an override return a conflict without changing inventory. Unassigning an IP removes its PTR override and host-community mappings on both backends.

Policy and community renames preserve IDs and refresh current references on both backends. Historical audit records retain their original names.

Startup seeds are explicit, atomic and idempotent; see [configuration](configuration.md) and [example catalog](../seeds.example.toml). Existing entries are preserved. No name such as `isolated` is implicitly created or protected.
