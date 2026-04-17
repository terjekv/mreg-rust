# Domain Catalog

## Core Inventory

- `Host`: canonical hostname, TTL override, comment, zone linkage.
- `CreateHost`: command to create a host with name, optional zone/TTL, comment.
- `UpdateHost`: patch command with optional name (rename), TTL, comment, zone.
- `HostContact`: reusable contact record referenced by hosts.
- `CreateHostContact`: command with email, display_name, hosts list.
- `IpAddressAssignment`: typed host address allocation with family, network affinity, optional MAC metadata.
- `AssignIpAddress`: command to assign an IP (explicit or auto-allocated from network).
- `PtrOverride`: explicit reverse mapping override for an IP.
- `CreatePtrOverride`: command with host_name, address, optional target_name.
- `Network`: CIDR allocation boundary, capacity metadata, policy linkage.
- `CreateNetwork`: command with CIDR, description, reserved count.
- `ExcludedRange`: reserved or unusable address span inside a network.
- `CreateExcludedRange`: command with network, start_ip, end_ip, description.
- `NameServer`: reusable NS record target with optional TTL.
- `CreateNameServer`: command with name and optional TTL.
- `UpdateNameServer`: patch command with optional TTL (double-option for clearing).

## Zone Management

- `ForwardZone`: SOA/TTL defaults, serial number, authoritative nameserver set.
- `CreateForwardZone`: command with name, primary_ns, nameservers, email, SOA params.
- `UpdateForwardZone`: patch command for SOA params and nameserver list.
- `ReverseZone`: reverse namespace plus managed network CIDR.
- `CreateReverseZone`: command with name, optional network, primary_ns, nameservers, email, SOA params.
- `UpdateReverseZone`: patch command for SOA params and nameserver list.
- `ForwardZoneDelegation`: delegated child namespace within a forward zone.
- `CreateForwardZoneDelegation`: command with zone_name, delegation name, comment, nameservers.
- `ReverseZoneDelegation`: delegated reverse namespace within a reverse zone.
- `CreateReverseZoneDelegation`: command with zone_name, delegation name, comment, nameservers.

## Grouping And Metadata

- `Label`: reusable tag/fact surfaced to authz policies and filtering.
- `CreateLabel`: command with name and description.
- `UpdateLabel`: patch command with optional description.
- `HostGroup`: nested grouping of hosts plus owner-group facts.
- `CreateHostGroup`: command with name, description, hosts, parent_groups, owner_groups.
- `BacnetIdAssignment`: BACnet identifier assigned one-to-one to a host.
- `CreateBacnetIdAssignment`: command with bacnet_id and host_name.

## Network Policy

- `NetworkPolicyAttribute`: boolean policy attribute definition.
- `NetworkPolicy`: policy bundle with attribute values and community pattern.
- `CreateNetworkPolicy`: command with name, description, optional community_template_pattern.
- `Community`: policy-scoped community bound to a network.
- `CreateCommunity`: command with policy_name, network CIDR, name, description.
- `HostCommunityAssignment`: assignment of a host/IP pair into a community.
- `CreateHostCommunityAssignment`: command with host_name, address, policy_name, community_name.

## Host Policy

- `HostPolicyAtom`: named policy property (e.g., `autoconfigure`, `no-icmp`).
- `CreateHostPolicyAtom`: command with name and description.
- `UpdateHostPolicyAtom`: patch command with optional description.
- `HostPolicyRole`: named collection of atoms assignable to hosts, with label references.
- `CreateHostPolicyRole`: command with name and description.
- `UpdateHostPolicyRole`: patch command with optional description.
- Role membership managed via add/remove operations for atoms, hosts, and labels.
- See [host-policy.md](host-policy.md) for full API documentation.

## Resource Records

- `RecordTypeDefinition`: runtime-extensible RR definition with validation schema, rendering template, RFC profile, and behavior flags.
- `CreateRecordTypeDefinition`: command to define a new record type.
- `RecordRrset`: set-level DNS object carrying owner name, class, TTL, and optional anchor metadata.
- `RecordInstance`: generic RR instance referencing an RRSet and carrying either structured JSON data or raw RFC 3597 wire data.
- `CreateRecordInstance`: command to create a record (anchored, unanchored, or raw).
- `UpdateRecord`: patch command with optional TTL and data (re-validated against type schema).
- `RawRdataValue`: RFC 3597 wire-format RDATA with presentation format `\# <len> <hex>`.
- `ValidatedRecordContent`: enum of Structured (JSON) or RawRdata after validation.
- `ExistingRecordSummary`: lightweight summary for relationship validation.
- Built-in types (18): A, AAAA, NS, PTR, CNAME, MX, TXT, SRV, NAPTR, SSHFP, LOC, HINFO, DS, DNSKEY, CAA, TLSA, SVCB, HTTPS.
- Built-in RR semantics are tightened by RFC-aware validation rules documented in `docs/rr-standards.md`.

## Pagination, Sorting, and Filtering

- `PageRequest`: cursor-based page request with `after` (UUID cursor), `limit`, `sort_by`, `sort_dir`.
- `Page<T>`: storage-layer page result with items, total count, and next cursor.
- `PageResponse<T>`: API-layer serializable wrapper mapped from `Page<T>`.
- `SortDirection`: enum `Asc` | `Desc`.
- Entity-specific filter structs: `HostFilter`, `NetworkFilter`, `RecordFilter`, `HostContactFilter`, `HostGroupFilter`, `BacnetIdFilter`, `PtrOverrideFilter`, `NetworkPolicyFilter`, `CommunityFilter`, `HostCommunityAssignmentFilter`.
- See `docs/pagination-sort-filter.md` for full API documentation.

## Authorization

- `AuthorizerClient`: pluggable authorization client (AllowAll, DenyAll, or Treetop).
- `Principal`: user identity with groups.
- `Action`: operation identifier.
- `Resource`: typed resource with kind, id, and attributes.
- `AuthorizationRequest`: principal + action + resource bundle.
- `AuthorizationDecision`: Allow or Deny.
- See `docs/authorization.md` for the authorization model and `docs/authz-action-matrix.md` for the detailed action/resource mapping.

## Audit

- `HistoryEvent`: immutable audit trail entry with actor, resource, action, data, timestamp.
- `CreateHistoryEvent`: command to record an audit event.

## Operational Objects

- `TaskEnvelope`: queued/running/succeeded/failed/cancelled asynchronous job with idempotency key, payload, progress, and result.
- `CreateTask`: command to enqueue a task.
- `ImportBatch`: staged mixed-entity batch with validation report and commit summary.
- `ImportBatchSummary`: storage-facing summary for list/status flows.
- `CreateImportBatch`: command with items list.
- `ExportTemplate`: managed MiniJinja template definition with engine, scope, body, metadata.
- `CreateExportTemplate`: command to create a template.
- `ExportRun`: rendered export artifact with status, parameters, and output.
- `CreateExportRun`: command to trigger an export.

## Error Types

- `AppError`: unified error type with variants: Config (500), Validation (400), NotFound (404), Conflict (409), Forbidden (403), Authz (502), Unavailable (503), Internal (500).

## Storage Note

These domain types are consumed through the storage facade (`Storage` trait) rather than depending directly on backend-specific persistence models. Core identity and validation-sensitive fields are modeled as opaque value objects (`DnsName`, `Hostname`, `ZoneName`, `Ttl`, `SerialNumber`, etc.) rather than raw strings. The storage layer handles cascading side-effects atomically (record cleanup on host delete, serial bumps on record mutations, A/AAAA/PTR auto-creation on IP assignment).
