# DHCP and Host Attachments

## Overview

mreg manages DHCP configuration through **host attachments** — the binding between
a host and a network. An attachment represents a host's presence on a network,
optionally keyed by a MAC address. DHCP identifiers, IPv6 prefix reservations,
and community assignments are children of attachments.

The DHCP configuration is then rendered into ISC dhcpd or Kea configuration files
via the export templating system (see [export-templating.md](export-templating.md)).

## Data Model

```
Host
 └── Attachment (host + network + optional MAC)
      ├── IP Addresses (one or more)
      ├── DHCP Identifiers (client-id for v4, DUID for v6)
      ├── Prefix Reservations (IPv6 delegated prefixes)
      └── Community Assignments (network policy communities)
```

A host can have multiple attachments — one per network it's connected to, or
multiple per network when differentiated by MAC address.

## Workflow

### 1. Create a host with an IP (automatic attachment)

When an IP is assigned to a host — either inline during host creation or via a
separate call — an attachment is automatically created for the (host, network)
pair if one doesn't exist.

```bash
# Create host with IP inline (attachment auto-created)
curl -X POST http://localhost:8080/api/v1/inventory/hosts \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "web.example.org",
    "zone": "example.org",
    "comment": "Web server",
    "ip_addresses": [
      { "address": "10.0.1.50", "mac_address": "aa:bb:cc:dd:ee:ff" }
    ]
  }'

# Or assign IP separately (attachment also auto-created)
curl -X POST http://localhost:8080/api/v1/inventory/ip-addresses \
  -H 'Content-Type: application/json' \
  -d '{
    "host_name": "web.example.org",
    "address": "10.0.1.51",
    "mac_address": "aa:bb:cc:dd:ee:ff"
  }'
```

### 2. Create an attachment explicitly

You can also create attachments directly, without assigning an IP first:

```bash
curl -X POST http://localhost:8080/api/v1/inventory/hosts/web.example.org/attachments \
  -H 'Content-Type: application/json' \
  -d '{
    "network": "10.0.1.0/24",
    "mac_address": "aa:bb:cc:dd:ee:ff",
    "comment": "Primary NIC"
  }'
```

Response includes the attachment `id` (UUID) needed for subsequent operations.

### 3. Add DHCP identifiers

DHCP identifiers tell the DHCP server how to identify the client.

**IPv4 — client-id:**

```bash
curl -X POST http://localhost:8080/api/v1/inventory/attachments/{attachment_id}/dhcp-identifiers \
  -H 'Content-Type: application/json' \
  -d '{
    "family": 4,
    "kind": "client_id",
    "value": "01:aa:bb:cc:dd:ee:ff",
    "priority": 100
  }'
```

**IPv6 — DUID:**

```bash
curl -X POST http://localhost:8080/api/v1/inventory/attachments/{attachment_id}/dhcp-identifiers \
  -H 'Content-Type: application/json' \
  -d '{
    "family": 6,
    "kind": "duid_ll",
    "value": "00:03:00:01:aa:bb:cc:dd:ee:ff",
    "priority": 100
  }'
```

Supported DHCP identifier kinds:

| Kind | Family | Description |
|------|--------|-------------|
| `client_id` | IPv4 | DHCPv4 client identifier |
| `duid_llt` | IPv6 | DUID based on link-layer + time |
| `duid_en` | IPv6 | DUID based on enterprise number |
| `duid_ll` | IPv6 | DUID based on link-layer address |
| `duid_uuid` | IPv6 | DUID based on UUID |
| `duid_raw` | IPv6 | Raw DUID value |

The `priority` field determines which identifier is used when multiple are set
for the same family (lower number = higher priority).

**Auto-creation of DHCP identifiers:** By default, DHCP identifiers are NOT
auto-created when an IP is assigned with a MAC address. You can enable automatic
creation via two environment variables:

- `MREG_DHCP_AUTO_V4_CLIENT_ID=true` — When an IPv4 IP is assigned and the
  attachment has a MAC address, automatically create a `client_id` DHCP
  identifier with value `01:<mac>` and priority 1000 (if one doesn't already
  exist for that attachment).

- `MREG_DHCP_AUTO_V6_DUID_LL=true` — When an IPv6 IP is assigned and the
  attachment has a MAC address, automatically create a `duid_ll` DHCP
  identifier with value `00:03:00:01:<mac>` and priority 1000 (if one doesn't
  already exist for that attachment).

The high priority number (1000) ensures that explicitly created identifiers
(default priority 100) always take precedence over auto-created ones. The
"if one doesn't already exist" check prevents duplicates when multiple IPs
are assigned to the same attachment.

Both flags default to `false` to preserve backward compatibility.

**Without auto-creation:** When you assign an IP with a MAC address, the MAC
is stored on the attachment but no `client_id` identifier is created. For
IPv4, this is fine — the DHCP export renderer falls back to the attachment's
MAC address when no explicit `client_id` exists. For IPv6, there is no
fallback — you must explicitly create a DUID identifier or the attachment
will be excluded from DHCPv6 exports.

Summary of what you need for each protocol:

| Protocol | Minimum for DHCP export | Recommended |
|----------|------------------------|-------------|
| DHCPv4 | MAC address on attachment (auto-used as fallback) | Explicit `client_id` identifier, or enable `MREG_DHCP_AUTO_V4_CLIENT_ID` |
| DHCPv6 | Explicit DUID identifier (required, no fallback) | Enable `MREG_DHCP_AUTO_V6_DUID_LL`, or create DUID manually |

A typical setup for a dual-stack host (with auto-creation enabled):

```bash
# With MREG_DHCP_AUTO_V4_CLIENT_ID=true and MREG_DHCP_AUTO_V6_DUID_LL=true,
# creating a host with IPs and a MAC address will auto-create both identifiers:
curl -X POST http://localhost:8080/api/v1/inventory/hosts \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "web.example.org",
    "zone": "example.org",
    "comment": "Dual-stack web server",
    "ip_addresses": [
      { "address": "10.0.1.50", "mac_address": "aa:bb:cc:dd:ee:ff" },
      { "network": "fd00:1::/64", "allocation": "first_free", "mac_address": "aa:bb:cc:dd:ee:ff" }
    ]
  }'
# Both DHCPv4 client_id (01:aa:bb:cc:dd:ee:ff) and DHCPv6 duid_ll
# (00:03:00:01:aa:bb:cc:dd:ee:ff) are auto-created on the attachment.
```

A typical setup without auto-creation:

```bash
# 1. Create host with IPs (attachment auto-created with MAC)
curl -X POST http://localhost:8080/api/v1/inventory/hosts \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "web.example.org",
    "zone": "example.org",
    "comment": "Dual-stack web server",
    "ip_addresses": [
      { "address": "10.0.1.50", "mac_address": "aa:bb:cc:dd:ee:ff" },
      { "network": "fd00:1::/64", "allocation": "first_free", "mac_address": "aa:bb:cc:dd:ee:ff" }
    ]
  }'

# 2. DHCPv4 works immediately (MAC fallback)
#    No extra steps needed — the export will use hardware ethernet aa:bb:cc:dd:ee:ff

# 3. For DHCPv6, add a DUID identifier to the attachment
curl -X POST http://localhost:8080/api/v1/inventory/attachments/{attachment_id}/dhcp-identifiers \
  -H 'Content-Type: application/json' \
  -d '{
    "family": 6,
    "kind": "duid_ll",
    "value": "00:03:00:01:aa:bb:cc:dd:ee:ff",
    "priority": 100
  }'
```

### 4. Add IPv6 prefix reservations

For IPv6 prefix delegation (DHCPv6-PD):

```bash
curl -X POST http://localhost:8080/api/v1/inventory/attachments/{attachment_id}/prefix-reservations \
  -H 'Content-Type: application/json' \
  -d '{ "prefix": "fd00:1:2::/48" }'
```

Only IPv6 prefixes are allowed.

### 5. Assign IPs to an existing attachment

```bash
curl -X POST http://localhost:8080/api/v1/inventory/attachments/{attachment_id}/ip-addresses \
  -H 'Content-Type: application/json' \
  -d '{ "address": "10.0.1.52" }'
```

### 6. List attachments for a host

```bash
curl http://localhost:8080/api/v1/inventory/hosts/web.example.org/attachments
```

### 7. Get attachment detail (includes IPs, DHCP IDs, prefix reservations)

```bash
curl http://localhost:8080/api/v1/inventory/attachments/{attachment_id}
```

Response:

```json
{
  "id": "uuid",
  "host_id": "uuid",
  "host_name": "web.example.org",
  "network_id": "uuid",
  "network_cidr": "10.0.1.0/24",
  "mac_address": "aa:bb:cc:dd:ee:ff",
  "comment": "Primary NIC",
  "ip_addresses": [...],
  "dhcp_identifiers": [...],
  "prefix_reservations": [...]
}
```

## DHCP Matcher Logic

When rendering DHCP exports, the system resolves a **matcher** for each
attachment:

- **IPv4**: Uses the highest-priority `client_id` DHCP identifier if one exists,
  otherwise falls back to the attachment's MAC address.
- **IPv6**: Uses the highest-priority DUID identifier. No fallback — if no DHCPv6
  identifier is set, the attachment is excluded from DHCPv6 exports and a warning
  is emitted.

The pre-filtered lists `dhcp4_attachments` and `dhcp6_attachments` on each
network only include attachments with valid matchers.

## Generating DHCP Configuration

Use the export templating system to render DHCP configs:

```bash
# Create an export run using a built-in template
curl -X POST http://localhost:8080/api/v1/workflows/export-runs \
  -H 'Content-Type: application/json' \
  -d '{
    "template_name": "kea-dhcp4-full",
    "scope": "dhcp"
  }'

# Execute the pending task
curl -X POST http://localhost:8080/api/v1/workflows/tasks/run-next
```

Built-in DHCP templates:

| Template | Server | Protocol |
|----------|--------|----------|
| `kea-dhcp4-full` | Kea | IPv4 |
| `kea-dhcp6-full` | Kea | IPv6 |
| `isc-dhcpd4-full` | ISC dhcpd | IPv4 |
| `isc-dhcpd6-full` | ISC dhcpd | IPv6 |

Fragment variants (`kea-dhcp4-fragment`, etc.) are also available for embedding
into existing configs. See [export-templating.md](export-templating.md) for the
full template reference and context data.

## API Reference

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/inventory/hosts/{name}/attachments` | List attachments for a host |
| `POST` | `/inventory/hosts/{name}/attachments` | Create an attachment |
| `GET` | `/inventory/attachments/{id}` | Get attachment detail |
| `PATCH` | `/inventory/attachments/{id}` | Update MAC address or comment |
| `DELETE` | `/inventory/attachments/{id}` | Delete an attachment |
| `POST` | `/inventory/attachments/{id}/ip-addresses` | Assign IP to attachment |
| `POST` | `/inventory/attachments/{id}/dhcp-identifiers` | Add DHCP identifier |
| `DELETE` | `/inventory/attachments/{id}/dhcp-identifiers/{did}` | Remove DHCP identifier |
| `POST` | `/inventory/attachments/{id}/prefix-reservations` | Add IPv6 prefix reservation |
| `DELETE` | `/inventory/attachments/{id}/prefix-reservations/{rid}` | Remove prefix reservation |
