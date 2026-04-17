# Wildcard DNS Records

## Overview

Wildcard DNS records (`*.example.org`) are supported as **unanchored records** — they exist in the record store without requiring a host entity.

This follows RFC 4592: wildcards are DNS synthesis rules at the zone level, not infrastructure hosts. A wildcard doesn't have an IP address, contacts, or group memberships — it's purely a DNS owner name with associated records.

## Creating wildcard records

Create wildcard records using the generic `/records` endpoint with no `owner_kind`:

```json
POST /api/v1/dns/records
{
  "type_name": "TXT",
  "owner_name": "*.example.org",
  "data": {"value": "v=spf1 -all"}
}
```

The record is unanchored — it has no host, zone, or nameserver entity backing it. The `owner_name` is validated as a `DnsName` (which allows `*`) rather than a `Hostname` (which doesn't).

## Supported record types

Any record type with `owner_name_syntax: "dns_name"` supports wildcard owner names:

| Type | Wildcard support |
|------|-----------------|
| A, AAAA | Yes |
| CNAME | Yes |
| TXT | Yes |
| MX | Yes |
| SRV | Yes |
| NS | Yes |
| CAA | Yes |
| TLSA | Yes |
| SVCB, HTTPS | Yes |
| SSHFP | No (hostname syntax only) |
| LOC | No (hostname syntax only) |
| HINFO | No (hostname syntax only) |

## What wildcards are NOT

- **Not hosts** — `*.example.org` is not created via `POST /hosts`. Hosts use `Hostname` validation which rejects `*`.
- **Not zone-anchored** — wildcards are typically unanchored. They could be zone-anchored with `owner_kind: "forward_zone"` if desired.
- **Not auto-managed** — no A/AAAA records are auto-created for wildcards (that only happens for host IP assignments).

## Examples

### Wildcard MX (catch-all mail)

```json
POST /api/v1/dns/records
{
  "type_name": "MX",
  "owner_name": "*.example.org",
  "data": {"preference": 10, "exchange": "mail.example.org"}
}
```

### Wildcard A (catch-all web)

```json
POST /api/v1/dns/records
{
  "type_name": "A",
  "owner_name": "*.example.org",
  "data": {"address": "10.0.0.1"}
}
```

### Wildcard CNAME

```json
POST /api/v1/dns/records
{
  "type_name": "CNAME",
  "owner_name": "*.cdn.example.org",
  "data": {"target": "cdn-provider.example.net"}
}
```

## Querying wildcard records

Use the standard `/records` endpoint with filters:

```
GET /api/v1/dns/records?owner_name__startswith=*.
GET /api/v1/dns/records?owner_name=*.example.org
```

## Relationship to the old mreg

The old mreg supported wildcard hosts as actual host entities (`*.example.org` in the hosts table). In mreg-rust, wildcards are modeled more correctly as DNS records without host entities, following RFC 4592's conceptual model.
