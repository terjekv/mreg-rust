# Original mreg API compatibility

This document records the compatibility implemented when the original
mreg-rust API moved from `/api/v1` to `/api/v2`.

The comparison was made on 2026-07-11 against unioslo/mreg `master` commit
`bbe8fa73d00fb7e8b3b7e06114c6f7519eb56a14`, specifically:

- [`mreg/api/urls.py`](https://github.com/unioslo/mreg/blob/bbe8fa73d00fb7e8b3b7e06114c6f7519eb56a14/mreg/api/urls.py)
- [`mreg/api/v1/urls.py`](https://github.com/unioslo/mreg/blob/bbe8fa73d00fb7e8b3b7e06114c6f7519eb56a14/mreg/api/v1/urls.py)
- [`hostpolicy/api/v1/urls.py`](https://github.com/unioslo/mreg/blob/bbe8fa73d00fb7e8b3b7e06114c6f7519eb56a14/hostpolicy/api/v1/urls.py)
- [OpenAPI schema fix PR #628](https://github.com/unioslo/mreg/pull/628), head
  `29d39bf0bdf6f8cb993b39e1be51b71dd2219527` at the time of review

## Compatibility model

- The native mreg-rust contract is now `/api/v2` and its Rust modules live in
  `src/api/v2`.
- Read routes for which mreg-rust stores the required data are rendered
  directly in legacy response shapes.
- No `/api/v1` resource route redirects to v2. Even when the operation is
  similar, a redirect would expose v2 pagination, identifiers, validation, or
  status codes and would therefore not be the same answer.
- Known legacy operations with no honest v2 equivalent return `501 Not
  Implemented` and a JSON explanation. They are not redirected to a vaguely
  similar operation which could read or mutate the wrong data.
- Unknown paths continue to return `404 Not Found`.
- Legacy mutations which do not yet have a request-and-response adapter return
  501. Native v2's presence is diagnostic only and is never used as a redirect.

This is not yet complete wire compatibility. Every implemented v1 route has a
dedicated handler; the remaining known routes fail explicitly.

## Unversioned `/api` endpoints

| Original endpoint | Result | Notes |
|---|---|---|
| `POST /api/token-auth/` | Direct implementation | Accepts the original form-encoded username/password request and returns `{ "token": ... }`. |
| `POST /api/token-logout/` | Direct implementation | Revokes the current mreg-rust token and returns the legacy 200 status. |
| `GET /api/token-is-valid/` | Direct implementation | Authentication middleware accepts the original `Authorization: Token ...` scheme (and Bearer for migration); handler returns 200. |
| `GET /api/meta/user` | Partial direct implementation | Preserves the main object shape and identity/groups. Django flags, network-regex permissions, login history, and token last-used data do not exist in the Rust model. |
| `GET /api/meta/version` | Direct implementation | Returns `{ "version": ... }`. |
| `GET /api/meta/libraries` | Partial direct implementation | Reports the Rust implementation and major Actix/utoipa versions; Python/Django/libpq package reporting is not applicable. |
| `GET /api/meta/health/heartbeat` | Direct implementation | Returns Unix `start_time` and seconds of `uptime`. |
| `GET /api/meta/health/ldap` | 501 | Auth scopes do not expose a safe, public LDAP bind-health operation. |
| `GET /api/meta/metrics` | 501 | mreg-rust does not currently include a Prometheus registry/exporter. |

## Direct v1 resource adapters

The following GET families have direct handlers which query the Rust services
and serialize legacy DRF-style responses. No entry in this table is a redirect.

| Original v1 family | Direct coverage and caveats |
|---|---|---|
| `bacnet/ids/` | List and BACnet-ID detail. |
| `hosts/` | List, hostname detail, create/update/rename/delete, contacts, policy roles, PTR overrides, groups, and DNS records. Host renames preserve contact relationships. |
| `hostgroups/` | List/detail, create/delete, and nested group, host, and owner reads and mutations. Legacy IDs are stable synthetic values. |
| `ipaddresses/` | Collection/detail, assignment, address/MAC update, host move, and removal. Legacy IDs derive from the stable assignment UUID and remain stable across address/host changes. |
| `labels/` | Collection/detail/name lookup, create, rename/update, and delete. |
| `nameservers/` | List/detail keyed by name. |
| `ptroverrides/` | Collection, create/change/delete, host projection, and network-level projections. |
| DNS record families | CNAME/HINFO/LOC/MX/NAPTR/SSHFP/SRV/TXT collection, detail where used by the CLI, create, and delete adapters, rendered from polymorphic stored records. Exact collection filters prevent records of the same type from being mistaken for one another. |
| `networks/` | List/CIDR detail, create/update/delete, excluded-range mutations, lookup by IP, reserved/used/unused counts and lists, first/random unused address, host and PTR projections, policy assignment/filtering, per-network community limits, and community/member CRUD. |
| Forward/reverse zones | List/detail, forward-zone create/update/delete, nameserver replacement, forward-delegation create/list/comment/delete, hostname/delegation lookup, delegation detail by name, and generated BIND-style zone files. |
| Network and host policy | Network-policy and network-policy-attribute list/create/detail/update/delete, boolean policy-attribute membership with atomic replace semantics, protected attributes, community-template patterns, plus host-policy atom/role CRUD, rename, atom/host membership, labels, and reverse membership projections. |
| `history/` | Collection rendered from Rust audit records with legacy host, group, and host-policy relation projections. Old integer history item identity is synthesized. |

## Direct v1 read adapters

The following GET responses are built from the Rust domain and service layer;
they do not redirect to v2:

- host-specific contact lists;
- host-group group, host, and owner membership lists, including DRF-style
  pagination envelopes;
- network lookup by IP;
- network first/random unused address, reserved address list, used/unused
  counts and lists, used host mapping, and PTR override mappings;
- every `dhcphosts/...` export, including IPv4, IPv6, CIDR ranges, and the
  IPv6-by-IPv4/MAC projection;
- forward-zone lookup by hostname, including delegation detection;
- forward/reverse zone nameserver lists and delegation lookup by name;
- host-policy role atom and host membership lists;
- forward and reverse BIND-style `zonefiles/{name}` output.

These adapters retain the normal mreg-rust authentication and Treetop
authorization checks.

The v1 mutation layer also implements the stateful CLI paths for forward zones,
networks and excluded ranges, hosts and contacts, IPv4/IPv6 assignments (including
the original forced network/broadcast semantics), host groups, forward-zone
delegations, labels, host policy, PTR overrides, and the legacy DNS record types.
These call the Rust service layer directly and then emit the original status and
response shape; they do not round-trip through v2.

Collection adapters use original DRF page-number envelopes (`count`, `next`,
`previous`, `results`) rather than v2 cursor pagination. Original `Token`
authentication, legacy 404 bodies, and normal Treetop authorization are also
applied at the v1 boundary.

## Explicitly unavailable routes

Known but unadapted mutations return 501 instead of redirecting. Most are
implementation backlog rather than fundamentally impossible: they need legacy
payload validation, identifier resolution, mutation calls, and legacy response
status/body adapters. The genuinely unavailable information is narrower:

- legacy integer primary keys were not retained. V1 adapters synthesize stable
  opaque integer identifiers where the CLI needs identity; the literal Django
  primary-key values cannot be reproduced;
- `permissions/netgroupregex/...` (authorization is delegated to Treetop in
  mreg-rust and the Django permission model is not stored);
- Django-only user flags, login history, token last-used metadata, and exact
  Python library inventory;
- LDAP health and Prometheus metrics as noted above.

All other 501s should be treated as adapters still to implement, not as claims
that the system lacks the underlying data or operation. The pinned CLI suite now
reaches explicit 501s only for legacy permission management. Label information
also encounters 501 while asking for permissions
attached to a label; the label and host-policy portions of that response are
implemented.

## mreg-cli compatibility CI

The `mreg-cli` CI job pins upstream mreg-cli commit
`72e598d3602812fc61a2d3a248ac8f4385dfb118`, runs its 401-command recorded
testsuite, and compares recordings. Exact matches pass. A changed command is
accepted only when its requests contain an allowlisted explicit status (501 by
default). Because the suite is stateful, an unsupported mutation marks later
differences as unverified downstream behavior rather than false matches; a
difference before the first such mutation fails the job. A mutating CLI command
whose unsupported GET preflight prevents its POST/PATCH/DELETE also taints later
state. Permission commands are explicitly non-tainting, as permitted for this
compatibility job.

Run the same path locally with `scripts/run-mreg-cli-compat.sh`. The script
handles Linux host networking and Docker Desktop on macOS. The final recorded
run on 2026-07-11 completed all 401 commands with 388 exact command matches, 13
explicit 501 permission-data gaps, no commands left unverified, and no unexpected
recording differences. The comparison normalizes only volatile identity, time,
and address values, plus one redundant read-only label lookup whose repetition
depends on the CLI cache surviving a permission 501. User-visible output, HTTP
methods, status codes, response bodies, and resulting state otherwise match.

## OpenAPI and documentation

The v2 OpenAPI document advertises version `2.0.0` and every native path under
`/api/v2`. Swagger UI and the schema are available at the PR #628-compatible
locations `/docs/` and `/docs/schema`. The previous mreg-rust locations
`/swagger-ui/` and `/api-docs/openapi.json` remain aliases.

PR #628 also proposes `/docs/redoc` and a YAML schema. mreg-rust does not bundle
the ReDoc sidecar assets, and utoipa serves the schema as JSON, so `/docs/redoc`
and YAML serialization are not matched. JSON is an OpenAPI-native encoding of
the same document.
