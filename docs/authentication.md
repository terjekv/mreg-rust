# Authentication

This document describes how mreg-rust authenticates requests and how that differs from authorization.

Authentication answers "who is the caller?". Authorization answers "may that caller perform this action?". mreg-rust has an explicit authentication layer in front of the existing authorization flow. Authorization is documented separately in [authorization.md](authorization.md).

For the full request pipeline and module layout, see [architecture.md](architecture.md).

## Overview

The request flow is:

1. Authentication resolves the caller into a canonical `Principal`.
2. That principal is attached to the request.
3. Authorization evaluates the principal against Treetop, development bypass, or deny-all behavior.

Authentication is configured with `MREG_AUTH_MODE`.

Supported modes:

- `none`
- `scoped`

## Modes

### `none`

This is the current test and development mode.

- Identity is taken directly from `X-Mreg-User` and `X-Mreg-Groups`.
- `POST /api/v1/auth/login` is disabled.
- No bearer token is required.
- This mode is intended for tests, local development, and trusted proxy setups only.

Example:

```http
GET /api/v1/system/status
X-Mreg-User: alice
X-Mreg-Groups: ops,net
```

### `scoped`

In `scoped` mode, mreg-rust loads one or more named authentication scopes at startup from `MREG_AUTH_SCOPES_FILE`.

Each scope has:

- a unique scope name such as `local`, `ldap-primary`, or `remote-sso`
- a scope kind: `local`, `ldap`, or `remote`
- backend-specific settings

Clients log in with `username` in `scope:username` form:

- `local:admin`
- `ldap-primary:bob`
- `remote-sso:alice`

All authenticated sessions use the same mreg-issued bearer token format, regardless of backend type.

Important behavior:

- authenticated principals are namespace-aware, for example `id="admin", namespace=["mreg","local"]`
- authenticated groups are also namespace-aware, for example `id="ops", namespace=["mreg","local"]`
- a stable serialized principal key is derived from that identity, for example `mreg::local::admin`
- `X-Mreg-User` and `X-Mreg-Groups` are ignored for identity resolution
- protected endpoints require `Authorization: Bearer <token>`

## Scope kinds

### `local`

`local` scopes define static users directly in the scopes file.

Each local user has:

- `username`
- `password_hash`
- `groups`

`password_hash` must be an Argon2id PHC string.

### `ldap`

`ldap` scopes authenticate directly against LDAP with search -> bind -> group lookup.

Login flow:

1. mreg-rust looks up the user with the configured search base and filter.
2. mreg-rust binds as that user with the supplied password.
3. After a successful bind, mreg-rust performs group lookup.
4. mreg-rust canonicalizes the result and issues a local JWT.

LDAP support is compile-gated behind the `ldap` feature.

### `remote`

`remote` scopes delegate credential validation to an upstream login service.

Login flow:

1. Client sends username/password to `POST /api/v1/auth/login`.
2. mreg-rust forwards the request to the configured remote scope login URL.
3. The upstream service returns a JWT.
4. mreg-rust validates that upstream JWT.
5. mreg-rust extracts raw username, groups, and expiry from the upstream JWT.
6. mreg-rust canonicalizes the result and issues its own local JWT.

Important behavior:

- upstream JWTs are used only as login proof, not as request bearer tokens
- request bearer tokens are always mreg-issued in `scoped` mode
- local token expiry is capped by both `MREG_AUTH_TOKEN_TTL_SECONDS` and upstream `exp`
- exactly one upstream JWT verification source must be configured:
  - `jwks_url`
  - `jwt_public_key_pem`
  - `jwt_hmac_secret`

## Canonical identity model

In `scoped` mode:

Each successful login resolves to:

- principal id: raw username
- principal namespace: `["mreg", scope]`
- group ids: raw group names
- group namespace: `["mreg", scope]`
- principal key: serialized namespace plus id, for example `mreg::local::admin`
- group key: serialized namespace plus id, for example `mreg::local::ops`

The login input still uses `scope:username`, but that is only the login syntax. It is not the stored authenticated identity.

Examples:

- login input: `local:admin`
- authenticated principal: `id="admin", namespace=["mreg","local"]`
- principal key: `mreg::local::admin`

This avoids collisions between identities coming from different backends.

## HTTP API

### `POST /api/v1/auth/login`

Request body:

```json
{
  "username": "local:admin",
  "password": "secret",
  "service_name": "mreg",
  "otp_code": "123456"
}
```

Fields:

- `username`: required
- `password`: required
- `service_name`: optional
- `otp_code`: optional

`service_name` and `otp_code` are forwarded only to `remote` scopes. `local` and `ldap` scopes ignore them.

Success response:

```json
{
  "access_token": "eyJhbGciOi...",
  "token_type": "Bearer",
  "expires_at": "2026-04-16T14:00:00Z",
  "principal": {
    "id": "admin",
    "namespace": ["mreg", "local"],
    "key": "mreg::local::admin",
    "username": "admin",
    "groups": [
      {
        "id": "ops",
        "namespace": ["mreg", "local"],
        "key": "mreg::local::ops"
      },
      {
        "id": "net",
        "namespace": ["mreg", "local"],
        "key": "mreg::local::net"
      }
    ]
  },
  "auth_scope": "local",
  "auth_provider_kind": "local"
}
```

Behavior notes:

- in `none` mode, login is disabled
- in `scoped` mode, malformed or unknown `scope:username` values return `400`
- invalid credentials for a known scope return `401`

### `GET /api/v1/auth/me`

Returns the resolved principal for the current request.

In `scoped` mode, the response includes:

- namespace-aware principal
- stable principal key
- raw username
- namespace-aware groups
- `auth_scope`
- `auth_provider_kind`
- token expiry

In `none` mode, `auth_scope` and `auth_provider_kind` are `null`.

### `POST /api/v1/auth/logout`

Revokes the current bearer token.

- requires a valid bearer token
- returns `204 No Content` on success
- is not meaningful in `auth_mode=none`, because there is no bearer token to revoke

### `POST /api/v1/auth/logout-all`

Revokes all existing tokens for a principal.

Request body:

```json
{
  "principal_key": "mreg::local::admin"
}
```

Behavior:

- requires a valid bearer token
- is protected by the normal authorization layer
- is intended for admin or other explicitly authorized operators
- invalidates all tokens for the supplied canonical principal that were issued before the revocation cutoff

## Protected vs unauthenticated endpoints

In `scoped` mode, protected endpoints require:

```http
Authorization: Bearer <token>
```

These endpoints remain unauthenticated:

- `GET /api/v1/system/health`
- `GET /api/v1/system/version`
- `POST /api/v1/auth/login`

## Identity headers

`X-Mreg-User` and `X-Mreg-Groups` are only trusted in `auth_mode=none`.

In `scoped` mode:

- identity comes only from the bearer token
- `X-Mreg-User` and `X-Mreg-Groups` are ignored for identity resolution

This prevents clients from bypassing authentication by supplying forged headers.

## Configuration summary

See [configuration.md](configuration.md) for the full environment variable reference.

Common:

- `MREG_AUTH_MODE`
- `MREG_AUTH_TOKEN_TTL_SECONDS`
- `MREG_AUTH_JWT_SIGNING_KEY`
- `MREG_AUTH_JWT_ISSUER`
- `MREG_AUTH_SCOPES_FILE`

The scopes file defines one or more named `local`, `ldap`, or `remote` scopes.

Example:

```json
{
  "scopes": [
    {
      "name": "local",
      "kind": "local",
      "users": [
        {
          "username": "admin",
          "password_hash": "$argon2id$v=19$m=19456,t=2,p=1$...",
          "groups": ["ops", "net"]
        }
      ]
    },
    {
      "name": "remote-sso",
      "kind": "remote",
      "login_url": "https://auth.example/login",
      "jwt_issuer": "auth.example",
      "jwt_hmac_secret": "change-me"
    }
  ]
}
```

For a complete example, see [../auth-scopes.example.json](../auth-scopes.example.json).

## Current limitations

- Access tokens only. There is no refresh-token flow.
- There is no database-backed local-user store or user-management API yet.
- LDAP support is compile-checked, but should still be validated against a real LDAP environment before production use.

Single-token logout and `logout_all` are supported through a revocation store.

## Operational notes

- Group membership is embedded in the local token, so group changes take effect on re-login or token expiry.
- In `remote` scopes, operational trust is split:
  - the upstream service authenticates credentials
  - mreg-rust validates the returned upstream JWT
  - mreg-rust issues the request bearer token
- JWT `sub` and auth-session revocation use the derived principal key such as `mreg::local::admin`
- Authentication and authorization are configured independently. You can run `scoped` authentication together with Treetop authorization or with the current development authorization bypass.

## Related documents

- [architecture.md](architecture.md)
- [authorization.md](authorization.md)
- [configuration.md](configuration.md)
