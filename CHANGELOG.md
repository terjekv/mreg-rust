# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Configurable atomic startup seeding (`MREG_SEED_CONFIG_PATH`) for policy attributes and policies, labels, nameservers, and host-policy atoms and roles, with audit events and explicit protection configuration.
- Native policy attributes, ordered attribute membership, community template identifiers, network policy assignment and community limits, and validated update operations for inventory and policy resources.

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

- **Breaking (communities/database):** community creation, including imports and direct storage calls, requires the network's assigned policy and an available community slot. Policy removal/replacement and limit reductions cannot invalidate existing communities. Run migrations `00000000000004_network_policy_attribute_order` and `00000000000005_community_policy_invariants`; repair existing mismatches first as described in `docs/core-mutation-upgrade.md`.
- **Breaking (validation):** network-policy names are limited to 100 characters; policy and community descriptions must be nonblank; community template identifiers require 1–100 ASCII letters, digits, or underscores and must be unique. Correct invalid persisted data before upgrading; use `null` to clear optional patterns.
- **Breaking (Rust API/configuration):** new validated update commands and transactional store operations are required by custom storage implementations. Explicit `Config` literals must supply `seed_data: SeedData::default()`. Implement `TxStorage::lock_seed_data` for the lifetime of each seed transaction. Configure desired initial policy attributes in a seed TOML file and `MREG_PROTECTED_POLICY_ATTRIBUTES`; no attribute name is implicitly created or protected.

- **Breaking (Rust API):** pagination requests now have private fields and use validated `PageLimit` values. Replace `PageRequest` literals and `deserialize_page_limit` with `PageRequest::new` and `Option<PageLimit>`; use `PageRequest::all()` for trusted internal enumeration. Host-policy membership APIs and role membership vectors now require `Hostname`, `LabelName`, and `HostPolicyName` instead of strings.
- **Breaking (validation):** constrained DNS, inventory, and policy request values now validate and normalize during JSON/path/query extraction, before handler authorization or lookup. Clients must handle HTTP 400 for invalid values rather than relying on later 403/404 responses, and authorization policies must match canonical names and addresses. Valid JSON and PATCH shapes remain compatible. Persisted invalid import batches, record schemas, and oversized raw RDATA must be corrected before loading; no database schema migration is required.
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

- Automatic IP allocation rejects frozen networks before creating assignments or attachments, including reuse of explicit attachments. Memory host deletion also removes attachment lookup entries.
- Adding nameservers to a memory-backed delegation advances the parent SOA serial; unchanged nameservers and comment-only edits preserve it.
- PostgreSQL label updates atomically target the original label and return NotFound when it does not exist, without modifying another label at the requested new name.
- Memory IP moves preserve the assignment's original creation timestamp across address, attachment and host changes.
- Policy attribute names validate and normalize during JSON/path extraction, before authorization or lookup. Invalid attribute membership and rename requests consistently return HTTP 400, with primitive wire shapes and PATCH semantics preserved.

- Delegation updates retain identity, DS records and glue, change only the delegation's NS records, and leave zone serials unchanged for comment-only edits.
- IP moves honor explicit attachments and their ownership, MAC, network and allocation constraints on both backends; moves with PTR overrides return a conflict until the override is explicitly removed.
- PostgreSQL IP reads and authorization retain the network selected by the persisted attachment when networks overlap.
- Memory policy/community renames refresh dependent community and assignment names. IP unassignment removes PTR overrides and host-community assignments consistently with PostgreSQL.
- PostgreSQL serializes community creation against the network's policy and limit, including concurrent requests; database triggers also enforce those invariants for direct SQL.

- Fixed intermittent PostgreSQL DNS reads failing under concurrent imports: current built-in record definitions are no longer rewritten on reads, and initialization or refresh is serialized within a transaction.
- Reduced memory-backend host-filter and record-listing work by evaluating address conditions once per inventory and cloning only the requested page.
- **Breaking (import batches):** bulk imports consistently generate eligible managed A/AAAA/PTR records and zone-apex NS records across supported storage backends, using the same creation paths as normal API calls. Zone creation also backfills records for earlier assignments in the batch, and generated records participate in rollback and assignment cleanup. Imported hosts retain their configured TTL. Imports without relevant DNS zones skip DNS-generation work. Import batches must omit explicit copies of synthesized records; previously completed imports need separate DNS verification or a fresh import into an empty destination. No database migration is required.
- **Breaking (pagination API):** public page sizes, including `18446744073709551615`, now remain capped at 1000; clients using that value for unbounded listing must follow pagination cursors. Internal fetch-all requests remain explicit.
- Closed constructor-validation bypasses in deserialization of import batches/items, record field/type schemas, and raw RDATA.
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
