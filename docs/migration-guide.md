# Migration Guide: Django mreg → mreg-rust

This guide covers migrating from the original Django-based mreg to mreg-rust. It addresses data migration, API changes, client updates, and operational considerations.

## Pre-migration checklist

- [ ] mreg-rust deployed with PostgreSQL backend
- [ ] Migrations run (`MREG_RUN_MIGRATIONS=true`)
- [ ] Reverse proxy configured (both old and new can run in parallel)
- [ ] `MREG_ALLOW_DEV_AUTHZ_BYPASS=true` set during migration (or Treetop configured)
- [ ] Test database populated and verified

## Data migration

### Approach: API-based import

The recommended migration path is to export from the old mreg API and import into mreg-rust using the bulk import endpoint (`POST /api/v1/workflows/imports`).

**Migration order** (respects foreign key dependencies):

1. **Nameservers** → `POST /dns/nameservers`
2. **Forward zones** → `POST /dns/forward-zones` (auto-creates NS records)
3. **Reverse zones** → `POST /dns/reverse-zones`
4. **Networks** → `POST /inventory/networks`
5. **Excluded ranges** → `POST /inventory/networks/excluded-ranges`
6. **Labels** → `POST /inventory/labels`
7. **Hosts** → `POST /inventory/hosts`
8. **IP addresses** → `POST /inventory/ip-addresses` (auto-creates A/AAAA + PTR records)
9. **DNS records** (CNAME, MX, TXT, SRV, etc.) → `POST /dns/records`
10. **Host contacts** → `POST /inventory/host-contacts`
11. **Host groups** → `POST /inventory/host-groups`
12. **Network policies** → `POST /policy/network/policies`
13. **Communities** → `POST /policy/network/communities`
14. **Host policy atoms** → `POST /policy/host/atoms`
15. **Host policy roles** → `POST /policy/host/roles` + atom/host/label assignments
16. **Delegations** → `POST /dns/forward-zones/{name}/delegations`

### Using the batch import endpoint

For bulk operations, use the import endpoint which provides atomicity:

```json
POST /api/v1/workflows/imports
{
  "requested_by": "migration-script",
  "items": [
    {"ref": "ns1", "kind": "nameserver", "operation": "create", "attributes": {"name": "ns1.example.org"}},
    {"ref": "zone1", "kind": "forward_zone", "operation": "create", "attributes": {
      "name": "example.org", "primary_ns": "ns1.example.org",
      "nameservers": ["ns1.example.org"], "email": "hostmaster@example.org"
    }},
    {"ref": "host1", "kind": "host", "operation": "create", "attributes": {
      "name": "web.example.org", "zone_ref": "zone1", "comment": "web server"
    }}
  ]
}
```

### What migrates automatically

When you assign an IP address to a host:
- **A or AAAA record** is auto-created in the record store
- **PTR record** is auto-created if a reverse zone exists for the IP's network

When you create a zone:
- **NS records** are auto-created for each nameserver

You do NOT need to manually create A, AAAA, NS, or PTR records — they are synthesized from the structural data.

### What needs explicit migration

- CNAME, MX, TXT, SRV, NAPTR, SSHFP, LOC, HINFO, CAA, TLSA records → `POST /records`
- HINFO is now a record type, not a host field (see below)
- Host contacts are now separate entities, not a text field on the host
- Wildcard hosts (`*.example.org`) become unanchored records, not host entities

## API differences summary

See [api-differences.md](api-differences.md) for the full comparison. Key changes:

| Old mreg | mreg-rust | Notes |
|----------|-----------|-------|
| `/zones/forward/` | `/forward-zones` | Kebab-case paths |
| `/zones/reverse/` | `/reverse-zones` | |
| `/ipaddresses/` | `/ip-addresses` | |
| `/cnames/`, `/txts/`, etc. | `/records` | Unified record endpoint |
| `/hostgroups/` | `/host-groups` | |
| `/hostpolicy/atoms/` | `/host-policy/atoms` | |
| `/ptroverrides/` | `/ptr-overrides` | |
| `?page=2&page_size=50` | `?limit=50&after=<cursor>` | Cursor pagination |
| `?ordering=-name` | `?sort_by=name&sort_dir=desc` | |
| `?name=foo` | `?name=foo` or `?name__contains=foo` | Operator-based filtering |

## Host response differences

### Old mreg host response (typical)

```json
{
  "id": 123,
  "name": "web.example.org",
  "zone": 45,
  "contact": "ops@example.org",
  "ttl": 3600,
  "comment": "web server",
  "hinfo": {"cpu": "x86_64", "os": "Linux"},
  "loc": null,
  "ipaddresses": [
    {"id": 789, "ipaddress": "10.0.0.10", "macaddress": "aa:bb:cc:dd:ee:ff"}
  ],
  "cnames": [{"name": "www.example.org", "ttl": 300}],
  "mxs": [],
  "txts": [{"txt": "v=spf1 -all"}]
}
```

### mreg-rust host response

```json
{
  "id": "uuid-here",
  "name": "web.example.org",
  "zone": "example.org",
  "ttl": 3600,
  "comment": "web server",
  "created_at": "2024-01-01T00:00:00Z",
  "updated_at": "2024-01-01T00:00:00Z"
}
```

**Key differences:**
- UUIDs instead of integer IDs
- Zone is a name string, not an integer FK
- No inline IP addresses, records, or contacts — these are separate API calls
- HINFO and contact are not fields on the host — they're separate entities
- Timestamps are included

**To get the full picture of a host, query:**
- `GET /inventory/hosts/{name}` — basic host info
- `GET /inventory/hosts/{name}/ip-addresses` — assigned IPs
- `GET /dns/records?owner_name={name}` — all DNS records
- `GET /inventory/host-contacts?host={name}` — contacts
- `GET /inventory/host-groups?host={name}` — group memberships

## Export template context

When rendering zone files via export templates, the following data is available in the MiniJinja context:

### Zones
```
forward_zones[].name, .primary_ns, .nameservers[], .email,
  .serial_no, .refresh, .retry, .expire, .soa_ttl, .default_ttl, .updated
reverse_zones[].name, .network, .primary_ns, .nameservers[], .email,
  .serial_no, .refresh, .retry, .expire, .soa_ttl, .default_ttl, .updated
forward_zone_delegations[].name, .zone_id, .nameservers[], .comment
reverse_zone_delegations[].name, .zone_id, .nameservers[], .comment
```

### Hosts and IPs
```
hosts[].name, .zone, .comment
ip_addresses[].address, .family
```

### Records
```
records[].type_name, .dns_type, .owner_name, .data, .raw_rdata, .rendered
rrsets[].type_name, .owner_name, .ttl
record_types[].name, .built_in
```

### Other
```
labels[].name, .description
nameservers[].name, .ttl
networks[].cidr, .description, .reserved
scope, parameters (from the export run)
```

### Example: Forward zone file template

```jinja
$ORIGIN {{ zone.name }}.
$TTL {{ zone.default_ttl }}
@ {{ zone.soa_ttl }} IN SOA {{ zone.primary_ns }}. {{ zone.email | replace("@", ".") }}. (
    {{ zone.serial_no }}  ; serial
    {{ zone.refresh }}     ; refresh
    {{ zone.retry }}       ; retry
    {{ zone.expire }}      ; expire
    {{ zone.soa_ttl }}     ; minimum
)

{% for record in records %}
{% if record.owner_name is ending_with("." ~ zone.name) %}
{{ record.owner_name }}. {{ record.rendered }}
{% endif %}
{% endfor %}
```

## Client migration

### For mreg-cli users

The existing mreg-cli will not work directly against mreg-rust due to different URL paths and payload formats. Options:

1. **Wait for `/api/compat/`** — a compatibility layer is planned (see [api-compatibility.md](api-compatibility.md))
2. **Use the new API directly** — curl, httpie, or a new CLI
3. **Use Swagger UI** — available at `/swagger-ui/` for interactive exploration

### Authentication changes

Old mreg used Django REST framework token auth. mreg-rust now supports:
- `none`: trusted `X-Mreg-User` and `X-Mreg-Groups` headers
- `scoped`: named `local`, `ldap`, or `remote` auth scopes with login as `scope:username` and mreg-issued JWT access tokens for namespace-aware principals

Authorization is still delegated to Treetop (or bypassed in dev mode). See [authentication.md](authentication.md) for the operational details.

## Operational considerations

### Running in parallel

During migration, both old and new mreg can run simultaneously against separate databases. Use a reverse proxy to route traffic:
- `/api/v1/` → mreg-rust (new clients)
- `/api/compat/` → mreg-rust compat layer (old clients, when implemented)
- Old mreg stays on its existing URL until fully migrated

### Zone serial numbers

mreg-rust uses YYYYMMDDNNNN format (12 digits, 10,000 changes per day). If your old mreg uses a different serial format, the first serial bump after migration will jump to the new format. This is safe — DNS secondaries only care that the serial increases.

### Audit trail

The old mreg's audit history is not automatically migrated. mreg-rust starts with a fresh audit trail. The old history should be archived separately.

### DNSSEC

If you use DNSSEC, DS/DNSKEY records can be stored in mreg-rust (with RFC 8624 validation), but key lifecycle management (generation, rollover, signing) is not implemented. Continue using your existing signing infrastructure.
