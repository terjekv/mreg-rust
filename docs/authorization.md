# Authorization

This document describes how mreg-rust authorizes requests after authentication has established a canonical principal.

Authentication answers "who is the caller?". Authorization answers "may that caller perform this action on this resource?". The authentication layer is documented in [authentication.md](authentication.md).

For the full request pipeline and module layout, see [architecture.md](architecture.md).

## Overview

The request flow is:

1. Authentication resolves the caller into a canonical `Principal`.
2. The principal is attached to the request.
3. Authorization evaluates that principal against Treetop, the development bypass, or deny-all behavior.

mreg-rust does not keep a second internal permission model alongside Treetop. Its job is to construct stable authorization requests and enforce the resulting allow or deny decision.

## Authorization modes

Authorization is controlled independently from authentication.

| Configuration | Behavior |
|---|---|
| `MREG_TREETOP_URL` set | All authorization decisions are delegated to Treetop. |
| No URL + `MREG_ALLOW_DEV_AUTHZ_BYPASS=true` | All authorization checks are allowed. This is development-only behavior. |
| No URL + bypass disabled | All authorization checks are denied. |

See [configuration.md](configuration.md) for the environment variables that control this.

## Principal mapping

The authz layer consumes the resolved principal from request extensions. That principal comes from:

- `X-Mreg-User` and `X-Mreg-Groups` only in `auth_mode=none`
- the validated mreg-issued bearer token in `auth_mode=scoped`

When no authenticated principal exists and `MREG_ALLOW_DEV_AUTHZ_BYPASS=true`, a default `anonymous` principal is used.

Implementation:

- principal extraction: [src/authz/mod.rs](../src/authz/mod.rs)
- authn middleware: [src/middleware/authn.rs](../src/middleware/authn.rs)

In `auth_mode=scoped`, the principal is namespace-aware both internally and at the Treetop boundary:

- principal id: `admin`
- principal namespace: `["mreg", "local"]`
- principal key: `mreg::local::admin`
- group id: `ops`
- group namespace: `["mreg", "local"]`

Authorization should key on namespace plus id, not on a string-encoded `scope:username` identifier.

## Action and resource model

Every authorization request sends:

- `principal`
- `action`
- `resource.kind`
- `resource.id`
- `resource.attrs`

The rules are:

- use stable dotted action IDs such as `host.create` or `zone.forward.delete`
- keep resource attrs minimal and action-local
- avoid sending full transitive graphs when a few attrs are enough for policy
- batch related checks when one HTTP request needs multiple auth decisions

Action constants and resource kinds live in [src/authz/actions.rs](../src/authz/actions.rs).

The detailed action catalog and expected attrs live in [authz-action-matrix.md](authz-action-matrix.md).

## Rust API surface

The intended authz wiring in handlers is request-first:

- `AuthorizationRequest`
- `AuthorizationRequest::builder(...)`
- `require_permission(authz, request)`
- `require_permissions(authz, requests)`

This supports:

- stable action constants instead of ad hoc strings
- per-action attrs
- batched authorization for multi-field updates and composite operations

## Batch authorization

Handlers that need more than one decision for a single HTTP request should batch them through `require_permissions()`.

Examples:

- `PATCH /hosts/{name}` when multiple fields change
- role membership mutations that need more than one action check
- future workflows that touch several resources atomically

The expected pattern is:

1. compute the required dotted actions
2. build one `AuthorizationRequest` per action
3. send them as one Treetop batch
4. reject the HTTP request if any decision is deny

## Failure policy

- policy deny becomes `403 Forbidden`
- upstream authorization transport or response failures become `502 Bad Gateway`
- health and version are intentionally unauthenticated, but `status` remains policy-controlled
- background workflows should preserve enough context to rebuild authz requests if they later need replay or follow-up checks

## Special cases

### Host authorization

For host authorization, `networks` is the canonical network context attribute and should always be sent as a set, even when empty. Network-scoped policies should key off `resource.networks.contains(...)`.

### Logout-all

`POST /api/v1/auth/logout-all` is an authorization-controlled operation.

- action: `auth_session.logout_all`
- resource kind: `auth_session`
- resource id: the principal whose sessions are being revoked

This is intended for admin or other explicitly authorized operators.

## Reference material

- Architecture: [architecture.md](architecture.md)
- Authentication: [authentication.md](authentication.md)
- Configuration: [configuration.md](configuration.md)
- Action matrix: [authz-action-matrix.md](authz-action-matrix.md)
- Treetop integration and request builders: [src/authz/mod.rs](../src/authz/mod.rs)
- Action constants: [src/authz/actions.rs](../src/authz/actions.rs)
