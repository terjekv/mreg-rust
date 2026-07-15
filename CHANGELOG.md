# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A dedicated Django-mreg v1 compatibility API, including the stateful endpoint
  adapters required by the pinned `mreg-cli` testsuite and an allowlisted CI
  comparison job for explicitly unsupported permission data.
- Network-policy attribute entities, protected attributes, community-template
  patterns, and atomic policy-attribute membership management in both storage
  backends and the native API.
- Core DNS management with forward zones, reverse zones, zone delegations, nameservers, and hosts with IP address management.
- DNS record system supporting 25 built-in record types (A, AAAA, NS, PTR, CNAME, MX, TXT, SRV, NAPTR, SSHFP, LOC, HINFO, DS, DNSKEY, CDS, CDNSKEY, CSYNC, CAA, TLSA, SVCB, HTTPS, DNAME, OPENPGPKEY, SMIMEA, URI) with RFC validation, plus runtime-defined types via RFC 3597 raw RDATA.
- Network management with CIDR networks, VLANs, reserved ranges, and used/unused address listing.
- Host policy system with atoms, roles, and role membership.
- Ancillary entities including host contacts, host groups, BACnet IDs, PTR overrides, network policies, and communities.
- Dual storage backends: in-memory (for testing) and PostgreSQL (for production) via a pluggable trait-based design.
- Export templating using MiniJinja-based template rendering with async task execution.
- Bulk import supporting JSON batch import with atomic execution.
- Authorization via Treetop-based permission checks on a per-action basis.
- Event system with domain event sinks (webhook, AMQP, Redis) and fire-and-forget delivery.
- API infrastructure with OpenAPI/Swagger UI, cursor-based pagination, operator-based filtering, and multi-field sorting.
- Observability through structured tracing with per-request spans and optional JSON log output.
- Service-layer audit recording for all mutations.

### Changed

- **Breaking:** The native mreg-rust API and OpenAPI paths moved from `/api/v1`
  to `/api/v2`; `/api/v1` now implements the original Django-mreg contract.
  Upgrade by changing native client base URLs and regenerated SDKs to `/api/v2`
  (including native auth and system endpoints) before deploying this release.
- Legacy wildcard hosts are represented as unanchored DNS owners instead of
  invalid native inventory hosts, so they remain manageable through v1 without
  weakening v2 hostname invariants.

### Fixed

- Kept v1 compatibility mutations isolated from native v2 semantics: host and
  delegation cleanup is identity-based, relationship and record moves preserve
  UUIDs where possible, and native duplicate-create conflict behavior remains
  non-destructive.
- Preserved noncanonical legacy NAPTR and SSHFP payloads without hidden sentinel
  values; v2 reports them with `legacy_compatibility` and omits canonical DNS
  rendering until the data is replaced with a valid native value.
