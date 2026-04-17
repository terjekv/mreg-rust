# Migration Backlog

## Deferred Until Fresh MVP Is Stable

- direct import from Django `mreg` database
- translation of legacy payload quirks to new API shapes (see [api-compatibility.md](api-compatibility.md) for the planned compat layer)
- replay of historical audit entries from legacy data
- message queue/event emission parity with Django signals

## Completed (Previously Deferred)

- ~~migration of hostpolicy roles/atoms~~ — implemented as `HostPolicyStore` with atoms, roles, and membership management (see [host-policy.md](host-policy.md))
- ~~compatibility shims for legacy clients~~ — planned as `/api/compat/` layer (see [api-compatibility.md](api-compatibility.md))

## Early Design Constraints

- keep stable natural keys where possible: host name, zone name, network CIDR, label name
- preserve enough metadata in the fresh model to support later ETL
- avoid internal IDs leaking into import/export contracts
