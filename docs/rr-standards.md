# RR Standards Notes

This project treats built-in DNS resource records as RFC-backed types rather than unstructured JSON blobs.

## Current enforcement

- `RRSet`s are first-class storage objects with owner name, class, anchor metadata, and TTL.
- RRSet TTLs must match for records with the same owner and type.
- Identical duplicate RRs are rejected inside the same RRSet.
- `CNAME` and `DNAME` are exclusive at an owner name and block other data at that owner (RFC 6672).
- `MX`, `SRV`, `NAPTR`, `NS`, and `PTR` target-like fields are checked against existing `CNAME`/`DNAME` owner names so alias targets can be rejected.
- `NS` target (`nsdname`) must not be an alias (RFC 2181).
- `PTR` is multi-valued (RFC 2181 Section 10.2). The `ptrdname` target must not be an alias.
- `MX` supports null MX semantics:
  - exchange `"."`
  - preference `0`
  - no other MX records in the same RRSet
- `TXT` is normalized to an array of DNS character-strings.
- `NAPTR` normalizes `service` input to RFC-correct `services` and requires exactly one of:
  - a non-empty `regexp`
  - a non-root `replacement`
- `SSHFP` validates algorithm and fingerprint type against the currently supported IANA-assigned values.
- `LOC` uses a structured payload with range/default validation.
- `DS` validates algorithm (RFC 8624 recommended: 8, 10, 13, 14, 15, 16) and digest type (2=SHA-256, 4=SHA-384).
- `DNSKEY` enforces protocol=3 (RFC 4034 Section 2.1.2) and validates algorithm against RFC 8624 recommendations.
- `CDS` validates identically to `DS` — algorithm and digest type per RFC 8624.
- `CDNSKEY` validates identically to `DNSKEY` — protocol=3 and algorithm per RFC 8624.
- `SMIMEA` validates identically to `TLSA` — usage (0-3), selector (0-1), matching_type (0-2) per RFC 8162.
- `CAA` validates tag format (RFC 8659: lowercase ASCII alphanumeric) and flags range (0-255).
- `TLSA` validates usage (0-3), selector (0-1), and matching_type (0-2) per RFC 6698.
- `SVCB` and `HTTPS` (RFC 9460) support priority + target + optional params. Target must not be an alias.
- Runtime-defined record types can opt into RFC 3597 raw wire-format RDATA using `behavior_flags.rfc3597.allow_raw_rdata`.
- Record ownership is flexible. A record may be:
  - anchored to a host, zone, delegation, or nameserver
  - unanchored and owned only by a DNS owner name

## Owner name validation

Owner names are validated per record type via the `owner_name_syntax` field in the RFC profile:

- `dns_name` — general DNS name syntax (allows underscored labels like `_dmarc`, `_acme-challenge`, DKIM selectors). Used by all types except SSHFP, LOC, HINFO.
- `hostname` — restricted to hostname syntax (no underscores). Used by: SSHFP, LOC, HINFO (host-specific record types).

## Built-in record types (25)

| Type | dns_type | Cardinality | Owner syntax | Alias-checked fields | Key RFC |
|------|----------|-------------|-------------|---------------------|---------|
| A | 1 | Multiple | dns_name | — | RFC 1035 |
| AAAA | 28 | Multiple | dns_name | — | RFC 3596 |
| NS | 2 | Multiple | dns_name | nsdname | RFC 1035, RFC 2181 |
| PTR | 12 | Multiple | dns_name | ptrdname | RFC 1035, RFC 2181 |
| CNAME | 5 | Single | dns_name | — | RFC 1034, RFC 2181 |
| DNAME | 39 | Single | dns_name | — | RFC 6672 |
| MX | 15 | Multiple | dns_name | exchange | RFC 1035, RFC 7505 |
| TXT | 16 | Multiple | dns_name | — | RFC 1035 |
| SRV | 33 | Multiple | dns_name | target | RFC 2782 |
| NAPTR | 35 | Multiple | dns_name | replacement | RFC 3403 |
| SSHFP | 44 | Multiple | hostname | — | RFC 4255, RFC 6594 |
| LOC | 29 | Single | hostname | — | RFC 1876 |
| HINFO | 13 | Single | hostname | — | RFC 1035 |
| DS | 43 | Multiple | dns_name | — | RFC 4034, RFC 8624 |
| DNSKEY | 48 | Multiple | dns_name | — | RFC 4034, RFC 8624 |
| CDS | 59 | Multiple | dns_name | — | RFC 7344, RFC 8078 |
| CDNSKEY | 60 | Multiple | dns_name | — | RFC 7344, RFC 8078 |
| CSYNC | 62 | Single | dns_name | — | RFC 7477 |
| CAA | 257 | Multiple | dns_name | — | RFC 8659 |
| TLSA | 52 | Multiple | dns_name | — | RFC 6698 |
| SMIMEA | 53 | Multiple | dns_name | — | RFC 8162 |
| SVCB | 64 | Multiple | dns_name | target | RFC 9460 |
| HTTPS | 65 | Multiple | dns_name | target | RFC 9460 |
| URI | 256 | Multiple | dns_name | — | RFC 7553 |
| OPENPGPKEY | 61 | Multiple | dns_name | — | RFC 7929 |

## Current storage model

- `rrsets`
  - owner name
  - DNS class
  - TTL
  - optional anchor metadata
- `records`
  - reference an RRSet
  - store either structured JSON payload or raw RFC 3597 wire bytes

## Intentional limits

- **DNS class IN only.** The `DnsClass` enum supports only `IN`. Classes CH (Chaosnet) and HS (Hesiod) are not used in modern DNS management and are not planned.
- **RRSIG, NSEC, NSEC3, CDS, CDNSKEY are not built-in types.** These are DNSSEC signing artifacts managed by signing infrastructure, not by a registry API. They can be created as runtime-defined types with RFC 3597 raw RDATA support if needed for storage/transport.
- **Delegation-backed record anchors** are fully implemented for both forward and reverse zones. Records can be anchored to delegations with scope validation (owner name must be within the delegation).

## Future work

- **DNSSEC key lifecycle management** — DS and DNSKEY records support validation (algorithm/protocol/digest per RFC 8624) but there is no key generation, signing, or rollover automation. This would be a major standalone feature.
- **RFC 3597 export tooling** — Raw RDATA records render correctly as `TYPE<N> \# <len> <hex>` in the export context. Additional MiniJinja template helpers for zone file formatting could improve the export experience for operators working with unusual record types.
