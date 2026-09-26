# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A dedicated Django-mreg v1 compatibility API, including the stateful endpoint
  adapters required by the pinned `mreg-cli` testsuite and an allowlisted CI
  comparison job documenting strict validation differences and unsupported
  permission data.
- Network-policy attribute entities, protected attributes, community-template
  patterns, and atomic policy-attribute membership management in both storage
  backends and the native API.
- EUI-48 and EUI-64 MAC address support across inventory APIs, storage backends, imports, and exports, with Ethernet-specific DHCP automation and matcher fallback limited to EUI-48 addresses.
- Core DNS management with forward zones, reverse zones, zone delegations, nameservers, and hosts with IP address management.
- DNS record system supporting 25 built-in record types (A, AAAA, NS, PTR, CNAME, MX, TXT, SRV, NAPTR, SSHFP, LOC, HINFO, DS, DNSKEY, CDS, CDNSKEY, CSYNC, CAA, TLSA, SVCB, HTTPS, DNAME, OPENPGPKEY, SMIMEA, URI) with RFC validation, plus runtime-defined types via RFC 3597 raw RDATA.
- Network management with CIDR networks, VLANs, reserved ranges, and used/unused address listing.
- Host policy system with atoms, roles, and role membership.
- Ancillary entities including host contacts, host groups, BACnet IDs, PTR overrides, network policies, and communities.
- Dual storage backends: in-memory (for testing) and PostgreSQL (for production) via a pluggable trait-based design.
- Export templating using MiniJinja-based template rendering with async task execution.
- Bulk import supporting JSON batch import with atomic execution.
- Authorization via Treetop-based permission checks on a per-action basis.
- Event system with webhook, AMQP, and Redis sinks backed by a transactional outbox and at-least-once retry delivery.
- API infrastructure with OpenAPI/Swagger UI, cursor-based pagination, operator-based filtering, and multi-field sorting.
- Observability through structured tracing with per-request spans and optional JSON log output.
- Service-layer audit recording for all mutations.

### Changed

- **Breaking (legacy compatibility):** v1 now enforces the same domain validation
  as v2. Correct invalid NAPTR/SSHFP data and blank network-policy or community
  descriptions before retrying requests. Explicit IP assignments must use
  allocatable space, and PTR overrides must refer to addresses owned by the
  supplied host. Clients must unfreeze networks before compatibility mutations.
  These rules also apply to direct service/storage callers; no compatibility
  validation bypass is available.
- **Breaking (validation):** network-policy names are limited to 100 characters.
  Community template identifiers must contain 1 to 100 ASCII letters, digits, or
  underscores. Correct invalid names and patterns before upgrading, and send
  `null` to clear an optional pattern.
- **Breaking (communities):** creating a community now requires its network to
  have the requested policy assigned and to be below its community limit. Assign
  `policy_name` on the network and adjust `max_communities` before creating new
  communities where necessary.
- **Breaking:** The native mreg-rust API and OpenAPI paths moved from `/api/v1`
  to `/api/v2`; `/api/v1` now implements the original Django-mreg contract.
  Upgrade by changing native client base URLs and regenerated SDKs to `/api/v2`
  (including native auth and system endpoints) before deploying this release.
- Legacy wildcard hosts are represented as unanchored DNS owners instead of
  invalid native inventory hosts, so they remain manageable through v1 without
  weakening v2 hostname invariants.
- **Breaking (Rust API):** `MacAddressValue::as_inner()` now returns `macaddr::MacAddr` instead of `macaddr::MacAddr6` so it can represent both EUI-48 and EUI-64 values. Callers that require a fixed width must migrate to `as_eui48()` or `as_eui64()` and handle `None`; callers that support both widths can match on `MacAddr::V6` and `MacAddr::V8`.
- **Breaking (API and database):** SOA record TTL is now `soa_record_ttl`; `negative_ttl` is only the RFC 2308 SOA minimum/negative-cache value. API clients and export templates must send/read both fields, and operators must run migration `00000000000003_enforce_domain_invariants` before starting the new server.
- **Breaking (API and Rust API):** DNS SOA serials are unsigned 32-bit RFC 1982 values. Values outside `0..=4294967295` are rejected; the migration reduces legacy oversized values modulo 2^32 and secondaries must be forced to perform a full refresh after upgrade.
- **Breaking (API):** pagination cursors are opaque keyset tokens instead of UUID row IDs. Clients must persist and return the token verbatim and must not reuse it with a different sort field or direction.
- **Breaking (API):** each IP assignment must specify exactly one of `address` (manual allocation, with the network inferred) or `network` (automatic allocation). Clients that sent both must omit `network` for manual assignments.
- **Breaking (validation):** DNS names, reverse-zone name/network pairs, built-in RDATA, RFC 3597 payload size, VLAN IDs (`1..=4094`), BACnet object instances (`0..=4194302`), DHCP identifiers, network reserved capacity, and attachment prefix reservations now reject previously accepted invalid values. Invalid persisted rows must be corrected before running the migration.
- **Breaking (DNS model):** RRset identity is scoped to an authoritative zone, allowing distinct parent-delegation and child-apex RRsets at a zone cut. Forward and reverse zones may no longer share a name.
- **Breaking (inventory):** frozen networks are immutable across their full attachment, IP, DHCP, prefix, excluded-range, and community graph. Unfreeze the network before mutating or deleting any of those resources.
- **Breaking (deletion):** deleting a zone or network no longer silently or broadly removes unrelated inventory. Zone deletion is rejected while hosts explicitly reference it; network deletion is rejected while attachment state remains.
- Managed A, AAAA, and PTR records are tracked explicitly per IP assignment, preserved separately from user-created records, and backfilled when a matching zone is created later.
- Event sinks now use durable at-least-once delivery. Consumers must deduplicate by event ID because retries, including partial multi-sink retries, can redeliver an event.
- Replaced the `iai-callgrind` benchmark harness with Gungraun 0.19.4 under `rust-pr-bench`; benchmark target names remain stable so the migration pull request retains base-versus-head measurements.

### Fixed

- Rejected host-group parent cycles and invalid PTR ownership during legacy
  replacements, and kept failed frozen-network mutations atomic across backends.
- Avoided linear in-memory attachment lookups and retained validated IP
  address values in host-filter indexes as inventories grow.
- Kept v1 compatibility mutations isolated from native v2 semantics: host and
  delegation cleanup is identity-based, relationship and record moves preserve
  UUIDs where possible, and native duplicate-create conflict behavior remains
  non-destructive.
- Fixed attachment-based IP imports in both storage backends: derive the host from the attachment, infer its network only for automatic allocation, and preserve the exact attachment when assigning an address. Direct IP imports also retain MAC addresses consistently across backends.
- Enforced RFC-correct CNAME/DNAME exclusivity and alias graphs, null MX semantics, RRset-wide TTL updates, authoritative owner containment, strict delegations, and zone serial bumps for generated records.
- Added canonical DNS master-file rendering for all 25 built-in record types, including absolute domain names, escaped character strings, LOC, DNSSEC records, SVCB/HTTPS parameters, and RFC 3597 raw RDATA.
- Corrected IPv4 `/31` and `/32` allocation semantics, full-width IPv6 capacity handling, exact attachment selection, overlapping excluded/prefix range checks, and allocation inside reserved or frozen space.
- Excluded IPv4 network identifiers even when `reserved=0`, enforced reserved capacity in PostgreSQL, and rejected pagination requests with `limit=0`.
- Updated DNSSEC validation to the current IANA zone-signing and digest registries, added exact singleton RFC 8078 CDS/CDNSKEY delete signaling, and updated SVCB validation to the current registered parameter keys including the reserved invalid key.
- Enforced CSYNC flags/type bitmaps, URI targets, DANE TLSA/SMIMEA owner names, OPENPGPKEY owner/data encoding, and the newly assigned TLSA C509 selector.
- Corrected the typed OpenAPI response shape for SVCB/HTTPS parameters so validated parameter arrays no longer fall back to opaque record data.
- Made host and network cascades backend-consistent, including managed DNS records, PTR overrides, BACnet IDs, contacts, groups, policy roles, attachments, and attachment children.
- Made seed/bootstrap operations idempotent and made sorted pagination stable across duplicate sort values and deletion of a page-boundary row.

### Security

- Authorization for host/IP, PTR override, and community-assignment mutations now uses persisted host, attachment, address, and network context instead of trusting caller-supplied relationship attributes.
- Export templates now use a restricted filter set, validate JSON output, and enforce output-size limits.
