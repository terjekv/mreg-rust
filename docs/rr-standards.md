# RR Standards Notes

This project treats built-in DNS resource records as RFC-backed types rather than unstructured JSON blobs.

## Current enforcement

- `RRSet`s are first-class storage objects with owner name, authoritative zone, class, anchor metadata, and TTL.
- RRSet identity and TTL consistency are zone-local. This permits distinct parent-delegation and child-apex NS data at a zone cut.
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
- `DS` validates zone-signing algorithms and assigned/private-use digest types against the current IANA registries, including the fixed digest lengths of assigned algorithms.
- `DNSKEY` enforces protocol=3 (RFC 4034 Section 2.1.2) and accepts only algorithms whose IANA registry entries permit zone signing.
- `CDS` otherwise validates like `DS`, but also accepts the exact RFC 8078 `0 0 0 00` delete signal as a singleton RRset.
- `CDNSKEY` otherwise validates like `DNSKEY`, but also accepts the exact RFC 8078 `0 3 0 AA==` delete signal as a singleton RRset.
- `CSYNC` accepts only currently assigned flag bits and validates its presentation-format RR type bitmap.
- `TLSA`, `SMIMEA`, and `OPENPGPKEY` enforce their RFC-defined underscored owner-name forms; OPENPGPKEY data must be non-empty base64.
- `URI` enforces its underscored service owner form and a non-empty absolute RFC 3986 URI target.
- `SMIMEA` validates identically to `TLSA` per RFC 8162.
- `CAA` canonicalizes tags to lowercase ASCII alphanumeric and accepts only flags `0` or `128`, because RFC 8659 defines only the issuer-critical bit.
- `TLSA` accepts currently assigned registry values (including C509 selector 2) and private-use value 255; fixed-length SHA-256/SHA-512 associations are length checked.
- `SVCB` and `HTTPS` (RFC 9460) support priority + target + optional params. Target must not be an alias.
- SVCB parameters are sorted by numeric key and validated for uniqueness, `mandatory` references, AliasMode, ALPN, port, and address-hint syntax.
- Built-in RDATA is rendered by type-aware DNS master-file renderers. Domain names are absolute and character strings are quoted/escaped; templates cannot accidentally change built-in wire meaning.
- Runtime-defined record types can opt into RFC 3597 raw wire-format RDATA using `behavior_flags.rfc3597.allow_raw_rdata`. Raw RDATA is limited to the DNS `RDLENGTH` maximum of 65,535 octets.
- SOA serials are unsigned 32-bit values and use RFC 1982 arithmetic. The SOA record TTL (`soa_record_ttl`) is distinct from the negative-cache/minimum field (`negative_ttl`).
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
  - authoritative zone UUID
  - DNS class
  - TTL
  - optional anchor metadata
- `records`
  - reference an RRSet
  - store either structured JSON payload or raw RFC 3597 wire bytes

## Intentional limits

- **DNS class IN only.** The `DnsClass` enum supports only `IN`. Classes CH (Chaosnet) and HS (Hesiod) are not used in modern DNS management and are not planned.
- **RRSIG, NSEC, and NSEC3 are not built-in types.** These signing artifacts are managed by signing infrastructure. They can be stored as runtime-defined RFC 3597 types when needed. CDS and CDNSKEY are built in because they are child-published signaling records used to update the parent delegation.
- **Delegation-backed record anchors** are fully implemented for both forward and reverse zones. Records can be anchored to delegations with scope validation (owner name must be within the delegation).

## Future work

- **DNSSEC key lifecycle management** — DS and DNSKEY records support registry-aware algorithm/protocol/digest validation, but there is no key generation, signing, or rollover automation. This would be a major standalone feature.
- **RFC 3597 export tooling** — Raw RDATA records render correctly as `TYPE<N> \# <len> <hex>` in the export context. Additional MiniJinja template helpers for zone file formatting could improve the export experience for operators working with unusual record types.
