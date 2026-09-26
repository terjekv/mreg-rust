# Type-Driven Design

## Goal

The core domain should reject invalid data as early as possible and make invalid states difficult to represent.
For this project, that means:

- private fields by default
- constructors and accessors instead of direct field mutation
- typed wrappers for DNS names, hostnames, zones, CIDRs, IP addresses, emails, MAC addresses, TTLs, and serial numbers
- primitive JSON compatibility at the HTTP boundary, with typed request fields and typed service/storage interfaces

## Current Value Objects

The `src/domain/types/` module currently provides opaque wrappers for:

- `DnsName`
- `Hostname`
- `ZoneName`
- `LabelName`
- `RecordTypeName`
- `EmailAddressValue`
- `MacAddressValue` (distinguishes EUI-48 from EUI-64 and exposes typed accessors for each)
- `Ipv4AddrValue`
- `Ipv6AddrValue`
- `IpAddressValue`
- `CidrValue`
- `Ttl`
- `SerialNumber`

Each type owns its normalization and validation rules.

### MAC address accessors

`MacAddressValue::as_inner()` returns `macaddr::MacAddr`, whose `V6` and `V8`
variants represent EUI-48 and EUI-64 respectively. Code that only accepts a
fixed-width address should use `as_eui48()` or `as_eui64()` and handle `None`.

This is a source-breaking change from the earlier EUI-48-only API, where
`as_inner()` returned `macaddr::MacAddr6`. Downstream Rust callers must migrate
to a typed accessor or match both `MacAddr` variants.

## Boundary Rules

- Requests may arrive as strings or primitive JSON values.
- Serde request fields, path extractors, and query extractors use existing domain types directly for context-independent validation and normalization.
- API handlers assemble typed commands, which enforce cross-field rules; storage enforces database-dependent rules atomically.
- JSON deserialization failures, malformed typed paths, and query validation failures use the application validation-error envelope. Payload-size failures retain HTTP 413.
- Authorization attributes use the canonical values accepted by the domain.
- Domain services and storage traits operate on typed values, not unchecked strings.
- Response payloads render typed values back into compatible string shapes.

## Privacy Rules

- Domain entities keep fields private and expose `restore`, `new`, and accessor methods.
- Persistence rows are separate from domain entities.
- Conversions from database rows into domain entities are fallible and must pass through validation.
- Transport DTOs should also avoid exposing unchecked internal fields when a conversion boundary is more explicit.

## Why This Matters Here

The original system has many fields that are stringly typed in practice. Rebuilding in Rust is an opportunity to move core invariants into the type system:

- canonical DNS rendering
- hostname restrictions separate from general DNS names
- CIDR validation and formatting
- typed TTL/serial ranges
- safer RR metadata and owner semantics

## Implementation Guidance

- Prefer a new value object over a raw `String` when a field has any durable format constraint.
- Keep third-party parser types behind project-defined wrappers.
- Do not expose parser crate types in storage or HTTP interfaces.
- Avoid convenience derives that bypass validation, including on entities with private fields. `ImportBatch`, `ImportItem`, `RecordFieldSchema`, `RecordTypeSchema`, and `RawRdataValue` deserialize through their validating constructors. Persisted JSON is not a trusted exception.
- Add tests for both accepted and rejected inputs whenever a new value object is introduced.

## Pagination contract

`PageLimit` accepts a positive size and caps it at 1000, including `u64::MAX`.
`PageRequest` keeps its fields private and exposes construction and read-only
accessors. An unlimited request is an explicit internal mode created only by
`PageRequest::all()`; HTTP query parameters cannot select it.

Rust callers must replace `PageRequest` struct literals with:

```rust
use mreg_rust::domain::pagination::{PageLimit, PageRequest, SortDirection};

let page = PageRequest::new(
    None,
    Some(PageLimit::new(50)?),
    Some("name".to_owned()),
    Some(SortDirection::Asc),
);
# Ok::<(), mreg_rust::errors::AppError>(())
```

For trusted internal enumeration, use `PageRequest::all()`, optionally followed
by `with_sort`. The old `deserialize_page_limit` helper is replaced by
`Option<PageLimit>` in query DTOs.

## Host-policy membership contract

Role membership uses `Hostname`, `LabelName`, and `HostPolicyName` across the API,
service facade, asynchronous storage traits, transactional storage traits, and
both backend implementations. `HostPolicyRole::restore` takes typed membership
vectors and its accessors return typed slices. PostgreSQL rows are parsed into
those types before restoration; response DTOs serialize the same string arrays.
Rust callers must construct the relevant name type instead of supplying a
`String` or `&str` to membership APIs.

## Builders and module organization

Use typestate when completing one step enables a meaningful next step, or when
it makes missing required data a compile-time error. Keep an ordinary builder
when it already requires all essential values in its constructor and subsequent
methods only add optional values. Validate remaining cross-field constraints in
its terminal method. `AuthorizationRequestBuilder` already requires principal,
action, resource kind, and resource ID; `UpdateAuthzBuilder` requires the request
and resource context. Neither needs marker states for optional attributes.

Keep behavior on the type that owns the invariant. For example, `AllocationPolicy`
deserializes its closed set of choices instead of each endpoint parsing strings.
Use ordinary `foo.rs` or `foo/mod.rs` discovery; do not introduce `#[path]`
overrides. Prefer imports to repeated fully qualified paths.

## Compatibility and upgrade

Valid HTTP payloads keep their primitive shapes, and PATCH fields continue to
support absent (unchanged), null (clear), and validated values (set). Invalid
constrained values now fail during extraction, before handler authorization or
resource lookup. Clients should handle HTTP 400 validation errors for these
inputs rather than depending on a later 403 or 404. Policies that compared raw,
noncanonical request spelling must use the normalized domain spelling.

There is no database schema migration for this change. Previously persisted
invalid import batches, record schemas, or oversized raw RDATA now fail to load;
correct that data using the same rules enforced for newly constructed values.
HTTP clients that used `limit=18446744073709551615` to fetch all records must
follow pagination cursors instead.
