# Type-Driven Design

## Goal

The core domain should reject invalid data as early as possible and make invalid states difficult to represent.
For this project, that means:

- private fields by default
- constructors and accessors instead of direct field mutation
- typed wrappers for DNS names, hostnames, zones, CIDRs, IP addresses, emails, MAC addresses, TTLs, and serial numbers
- string compatibility at the HTTP boundary, but typed values everywhere inside the service

## Current Value Objects

The `src/domain/types.rs` module currently provides opaque wrappers for:

- `DnsName`
- `Hostname`
- `ZoneName`
- `LabelName`
- `RecordTypeName`
- `EmailAddressValue`
- `MacAddressValue`
- `Ipv4AddrValue`
- `Ipv6AddrValue`
- `IpAddressValue`
- `CidrValue`
- `Ttl`
- `SerialNumber`

Each type owns its normalization and validation rules.

## Boundary Rules

- Requests may arrive as strings or primitive JSON values.
- API handlers convert those values into typed command objects immediately.
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
- Avoid convenience derives that bypass validation.
- Add tests for both accepted and rejected inputs whenever a new value object is introduced.
