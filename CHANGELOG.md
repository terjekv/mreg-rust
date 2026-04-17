# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
