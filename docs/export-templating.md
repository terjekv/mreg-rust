# Export Templating

## Overview

The export templating system renders structured data from the mreg domain into text
artifacts using MiniJinja templates. Typical use cases include generating BIND zone
files, inventory reports, and custom data extractions.

Execution is asynchronous and task-based: creating an export run queues a background
task, and a worker claims and executes it via the task runner endpoint. The rendered
output and artifact metadata are stored on the completed run.

## API Endpoints

All paths are relative to `/api/v1`.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/export-templates` | List all export templates (paginated) |
| `POST` | `/export-templates` | Create a new export template |
| `GET` | `/export-runs` | List all export runs (paginated) |
| `POST` | `/export-runs` | Create an export run (queues a task) |
| `POST` | `/tasks/run-next` | Claim and execute the next pending task |

The `POST /tasks/run-next` endpoint is shared with the import system. When it claims
a task with `kind: "export_run"`, it executes the template render and stores the
result on the run.

## Template Model

Each export template has the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID | Server-assigned identifier |
| `name` | string | Unique template name (case-insensitive) |
| `description` | string | Human-readable description |
| `engine` | string | Template engine; currently only `minijinja` |
| `scope` | string | Determines what data context is built (see Scopes) |
| `body` | string | The MiniJinja template source |
| `metadata` | JSON object | Arbitrary metadata stored alongside the template |
| `builtin` | boolean | `true` for system-provided templates, `false` for user-created |

All of `name`, `engine`, `scope`, and `body` are required and must be non-empty.

## Scopes

The scope determines which data is collected and made available as template context
variables. Four scopes are supported:

### `forward_zone`

Renders data for a single forward DNS zone. Requires a `zone_name` parameter.

### `reverse_zone`

Renders data for a single reverse DNS zone. Requires a `zone_name` parameter.

### `inventory`

Renders a global view across all zones, hosts, networks, and records. No required
parameters. Suitable for cross-cutting reports and inventory exports.

### `dhcp`

Renders an attachment-centric DHCP inventory grouped by network. No required
parameters. Suitable for canonical DHCP exports and backend-specific renderer
templates.

## Context Data Reference

This is the authoritative reference for what variables are available inside templates
for each scope. Field names match the JSON keys exactly.

### `forward_zone` scope

| Variable | Type | Description |
|----------|------|-------------|
| `zone` | object | The forward zone (see zone fields below) |
| `delegations` | array | Zone delegations |
| `records` | array | All DNS records belonging to this zone |
| `hosts` | array | Hosts in this zone |
| `ip_addresses` | array | IP addresses assigned to hosts in this zone |
| `scope` | string | The scope name (`"forward_zone"`) |
| `parameters` | object | The parameters passed when creating the run |

**`zone` fields:** `name`, `primary_ns`, `nameservers` (array), `email`,
`serial_no`, `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl`, `updated`.

**`delegations[]` fields:** `name`, `nameservers` (array), `comment`.

**`records[]` fields:** `owner_name`, `type_name`, `ttl` (nullable), `data`,
`rendered` (nullable; the pre-formatted RDATA string, with RFC 3597 fallback).

**`hosts[]` fields:** `id`, `name`, `ttl` (nullable), `comment`.

**`ip_addresses[]` fields:** `host_id`, `address`, `family`, `mac_address` (nullable).

### `reverse_zone` scope

| Variable | Type | Description |
|----------|------|-------------|
| `zone` | object | The reverse zone (see zone fields below) |
| `delegations` | array | Zone delegations |
| `records` | array | All DNS records belonging to this zone |
| `hosts` | array | Always `[]` (hosts are not associated with reverse zones) |
| `ip_addresses` | array | Always `[]` |
| `scope` | string | The scope name (`"reverse_zone"`) |
| `parameters` | object | The parameters passed when creating the run |

**`zone` fields:** `name`, `network` (nullable), `primary_ns`, `nameservers` (array),
`email`, `serial_no`, `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl`,
`updated`.

**`delegations[]` fields:** `name`, `nameservers` (array), `comment`.

**`records[]` fields:** `owner_name`, `type_name`, `ttl` (nullable), `data`,
`rendered` (nullable).

### `inventory` scope

The inventory scope provides a global view of the entire system.

| Variable | Type | Description |
|----------|------|-------------|
| `scope` | string | `"inventory"` |
| `parameters` | object | The parameters passed when creating the run |
| `labels` | array | All labels |
| `nameservers` | array | All nameservers |
| `forward_zones` | array | All forward zones |
| `forward_zone_delegations` | array | All forward zone delegations |
| `reverse_zones` | array | All reverse zones |
| `reverse_zone_delegations` | array | All reverse zone delegations |
| `networks` | array | All networks |
| `hosts` | array | All hosts |
| `ip_addresses` | array | All IP address assignments |
| `record_types` | array | All record types |
| `rrsets` | array | All RRsets |
| `records` | array | All records |

**`labels[]` fields:** `name`, `description`.

**`nameservers[]` fields:** `name`, `ttl` (nullable).

**`forward_zones[]` fields:** `name`, `primary_ns`, `email`, `serial_no`, `refresh`,
`retry`, `expire`, `soa_ttl`, `default_ttl`, `nameservers` (array).

**`forward_zone_delegations[]` fields:** `name`, `zone_id`, `comment`,
`nameservers` (array).

**`reverse_zones[]` fields:** `name`, `network` (nullable), `primary_ns`, `email`,
`serial_no`, `refresh`, `retry`, `expire`, `soa_ttl`, `default_ttl`,
`nameservers` (array).

**`reverse_zone_delegations[]` fields:** `name`, `zone_id`, `comment`,
`nameservers` (array).

**`networks[]` fields:** `cidr`, `description`, `reserved`.

**`hosts[]` fields:** `id`, `name`, `zone` (nullable), `ttl` (nullable), `comment`.

**`ip_addresses[]` fields:** `host_id`, `address`, `family`, `mac_address` (nullable).

**`record_types[]` fields:** `name`, `built_in`.

**`rrsets[]` fields:** `type_name`, `owner_name`, `ttl` (nullable).

**`records[]` fields:** `type_name`, `dns_type` (nullable), `owner_name`, `data`,
`raw_rdata` (nullable), `rendered` (nullable).

### `dhcp` scope

The DHCP scope is grouped around `host_attachment` identity. Top-level variables:

| Variable | Type | Description |
|----------|------|-------------|
| `scope` | string | `"dhcp"` |
| `parameters` | object | The parameters passed when creating the run |
| `warnings` | array | Validation/render warnings collected while building the context |
| `networks` | array | All managed networks with attachment data |
| `dhcp4_networks` | array | IPv4-only subset of `networks` |
| `dhcp6_networks` | array | IPv6-only subset of `networks` |

**`networks[]` fields:** `id`, `cidr`, `family`, `description`, `vlan`, `dns_delegated`,
`category`, `location`, `frozen`, `reserved`, `communities`, `attachments`,
`dhcp4_attachments`, `dhcp6_attachments`.

**`attachments[]` fields:** `id`, `host_id`, `host_name`, `mac_address`, `comment`,
`dhcp_identifiers`, `matchers`, `ip_addresses`, `ipv4_addresses`, `ipv6_addresses`,
`primary_ipv4_address`, `primary_ipv6_address`, `prefix_reservations`,
`community_assignments`.

**`matchers` fields:** `ipv4`, `ipv6`, each either `null` or an object with `kind`
and `value`. Built-in templates use these resolved matchers rather than repeating
fallback logic.

## Parameters

When creating an export run, the `parameters` field is an arbitrary JSON object
passed through to the template context as `{{ parameters }}`.

For zone scopes (`forward_zone` and `reverse_zone`), the `zone_name` key is
**required** inside `parameters`:

```json
{
  "template_name": "bind-forward-zone",
  "scope": "forward_zone",
  "parameters": { "zone_name": "example.com" }
}
```

For the `inventory` and `dhcp` scopes, parameters are optional and can carry any custom data
your template needs.

## Built-in Templates

Built-in templates are registered at startup. They cannot be overwritten by
user-created templates with the same name.

### `bind-forward-zone` (scope: `forward_zone`)

Renders a BIND-compatible forward zone file including SOA, all records, and
owner-name relativization (zone apex replaced with `@`).

```jinja
; Zone file for {{ zone.name }}
; Generated by mreg-rust
$ORIGIN {{ zone.name }}.
$TTL {{ zone.default_ttl }}

; SOA record
@ {{ zone.soa_ttl }} IN SOA {{ zone.primary_ns }}. {{ zone.email | replace("@", ".") }}. (
    {{ zone.serial_no }}  ; serial
    {{ zone.refresh }}     ; refresh
    {{ zone.retry }}       ; retry
    {{ zone.expire }}      ; expire
    {{ zone.soa_ttl }}     ; minimum TTL
)

; Records
{% for record in records %}
{% set owner = record.owner_name %}
{% if owner == zone.name %}{% set owner = "@" %}{% endif %}
{{ owner }}{% if owner != "@" %}.{% endif %} {{ record.ttl | default(value=zone.default_ttl) }} IN {{ record.type_name }} {{ record.rendered }}
{% endfor %}
```

### `bind-reverse-zone` (scope: `reverse_zone`)

Renders a BIND-compatible reverse zone file. Owner names are always written as
fully qualified (with trailing dot) since reverse zone entries are typically
in-addr.arpa or ip6.arpa names.

```jinja
; Reverse zone file for {{ zone.name }}
; Generated by mreg-rust
$ORIGIN {{ zone.name }}.
$TTL {{ zone.default_ttl }}

; SOA record
@ {{ zone.soa_ttl }} IN SOA {{ zone.primary_ns }}. {{ zone.email | replace("@", ".") }}. (
    {{ zone.serial_no }}  ; serial
    {{ zone.refresh }}     ; refresh
    {{ zone.retry }}       ; retry
    {{ zone.expire }}      ; expire
    {{ zone.soa_ttl }}     ; minimum TTL
)

; Records
{% for record in records %}
{{ record.owner_name }}. {{ record.ttl | default(value=zone.default_ttl) }} IN {{ record.type_name }} {{ record.rendered }}
{% endfor %}
```

### DHCP built-ins (scope: `dhcp`)

Nine built-in templates cover both Kea and ISC dhcpd, each in IPv4 and IPv6
variants, with **fragment** (embeddable into an existing config) and **full**
(standalone config file) forms.

| Template | Server | Protocol | Form |
|----------|--------|----------|------|
| `dhcp-canonical-json` | — | both | Raw JSON context dump (`json` engine) |
| `kea-dhcp4-fragment` | Kea | IPv4 | Reservations array |
| `kea-dhcp4-full` | Kea | IPv4 | Complete `Dhcp4` config |
| `kea-dhcp6-fragment` | Kea | IPv6 | Reservations array |
| `kea-dhcp6-full` | Kea | IPv6 | Complete `Dhcp6` config |
| `isc-dhcpd4-fragment` | ISC dhcpd | IPv4 | Host stanzas |
| `isc-dhcpd4-full` | ISC dhcpd | IPv4 | Full config with `authoritative;` |
| `isc-dhcpd6-fragment` | ISC dhcpd | IPv6 | Host stanzas |
| `isc-dhcpd6-full` | ISC dhcpd | IPv6 | Full DHCPv6 config |

#### DHCP matcher logic

Each attachment carries a `matchers` object with `ipv4` and `ipv6` fields.
These are pre-resolved by the export context builder so templates don't need
fallback logic:

- **IPv4 matcher**: Uses the highest-priority `client_id` DHCP identifier if
  one exists, otherwise falls back to the attachment's MAC address. The matcher
  object is `{"kind": "client_id", "value": "..."}` or
  `{"kind": "mac_address", "value": "..."}`.
- **IPv6 matcher**: Uses the highest-priority DUID identifier (`duid_llt`,
  `duid_en`, `duid_ll`, `duid_uuid`, or `duid_raw`). No fallback — if no
  DHCPv6 identifier is set, `matchers.ipv6` is `null` and a warning is emitted
  if the attachment has IPv6 addresses or prefix reservations.

The convenience lists `dhcp4_attachments` and `dhcp6_attachments` on each
network are pre-filtered to only include attachments that have a valid matcher
and at least one address (or prefix reservation for IPv6).

#### Kea DHCPv4 (`kea-dhcp4-full`)

```jinja
{
  "Dhcp4": {
    "interfaces-config": {
      "interfaces": []
    },
    "subnet4": [
{% for network in dhcp4_networks %}
      {
        "subnet": "{{ network.cidr }}",
        "reservations": [
{% for attachment in network.dhcp4_attachments %}
          {
{% if attachment.matchers.ipv4.kind == "client_id" %}
            "client-id": "{{ attachment.matchers.ipv4.value }}",
{% else %}
            "hw-address": "{{ attachment.matchers.ipv4.value }}",
{% endif %}
            "hostname": "{{ attachment.host_name }}",
            "ip-address": "{{ attachment.primary_ipv4_address }}"
          }{% if not loop.last %},{% endif %}
{% endfor %}
        ]
      }{% if not loop.last %},{% endif %}
{% endfor %}
    ]
  }
}
```

#### Kea DHCPv6 (`kea-dhcp6-full`)

```jinja
{
  "Dhcp6": {
    "interfaces-config": {
      "interfaces": []
    },
    "subnet6": [
{% for network in dhcp6_networks %}
      {
        "subnet": "{{ network.cidr }}",
        "reservations": [
{% for attachment in network.dhcp6_attachments %}
          {
            "duid": "{{ attachment.matchers.ipv6.value }}",
            "hostname": "{{ attachment.host_name }}"{% if attachment.primary_ipv6_address %},
            "ip-addresses": ["{{ attachment.primary_ipv6_address }}"]{% endif %}{% if attachment.prefix_reservations %},
            "prefixes": [{% for prefix in attachment.prefix_reservations %}"{{ prefix.prefix }}"{% if not loop.last %}, {% endif %}{% endfor %}]{% endif %}
          }{% if not loop.last %},{% endif %}
{% endfor %}
        ]
      }{% if not loop.last %},{% endif %}
{% endfor %}
    ]
  }
}
```

#### ISC dhcpd IPv4 (`isc-dhcpd4-full`)

```jinja
# Generated by mreg-rust
authoritative;

{% for network in dhcp4_networks %}
# {{ network.cidr }} {{ network.description }}
{% for attachment in network.dhcp4_attachments %}
host {{ attachment.host_name | replace(".", "-") }} {
  option host-name "{{ attachment.host_name }}";
{% if attachment.matchers.ipv4.kind == "client_id" %}
  option dhcp-client-identifier "{{ attachment.matchers.ipv4.value }}";
{% else %}
  hardware ethernet {{ attachment.matchers.ipv4.value }};
{% endif %}
  fixed-address {{ attachment.primary_ipv4_address }};
}

{% endfor %}
{% endfor %}
```

#### ISC dhcpd IPv6 (`isc-dhcpd6-full`)

```jinja
# Generated by mreg-rust

{% for network in dhcp6_networks %}
# {{ network.cidr }} {{ network.description }}
{% for attachment in network.dhcp6_attachments %}
host {{ attachment.host_name | replace(".", "-") }} {
  option host-name "{{ attachment.host_name }}";
  host-identifier option dhcp6.client-id {{ attachment.matchers.ipv6.value }};
{% if attachment.primary_ipv6_address %}
  fixed-address6 {{ attachment.primary_ipv6_address }};
{% endif %}
{% for prefix in attachment.prefix_reservations %}
  fixed-prefix6 {{ prefix.prefix }};
{% endfor %}
}

{% endfor %}
{% endfor %}
```

The **fragment** variants (`kea-dhcp4-fragment`, `isc-dhcpd4-fragment`, etc.)
contain only the reservation/host blocks without the outer config wrapper,
for embedding into an existing configuration file.

## Task-based Execution

Export runs use the shared task queue rather than executing inline. The lifecycle is:

1. **Create run** -- `POST /api/v1/workflows/export-runs` validates the request, looks up the
   named template, creates a task with `kind: "export_run"` and
   `payload: { "run_id": "<uuid>" }`, and returns the run with `status: "queued"`.

2. **Claim task** -- A worker calls `POST /api/v1/workflows/tasks/run-next`. The endpoint
   claims the next available task (any kind). If the claimed task is an
   `export_run`, it proceeds to step 3.

3. **Execute** -- The system builds the scope-appropriate context, renders the
   template via the selected engine (`minijinja` or `json`), stores the
   `rendered_output` and `artifact_metadata: { "bytes": <length>, "warnings": [...] }`
   on the run, and transitions both the run and task to `succeeded`.

4. **Retrieve result** -- The `run-next` response includes the full run object
   (with rendered output) in the `workflow_result` field. The run can also be
   found later via `GET /api/v1/workflows/export-runs`.

Run statuses: `queued`, `running`, `succeeded`, `failed`, `cancelled`.

If rendering fails, the task is marked `failed` with the error message, and the
error is returned to the caller.

See [task-system.md](task-system.md) for shared task queue behavior and
`run-next` worker semantics across workflows.

## Authorization

The following authorization actions apply to export operations:

| Action | Resource kind | When |
|--------|---------------|------|
| `export.template.list` | `export_template` | `GET /export-templates` |
| `export.template.create` | `export_template` | `POST /export-templates` |
| `export.run.list` | `export_run` | `GET /export-runs` |
| `export.run.create` | `export_run` | `POST /export-runs` |
| `worker.task.claim_next` | `task` | `POST /tasks/run-next` (claiming) |
| `worker.task.execute.export_run` | `task` | `POST /tasks/run-next` (executing an export) |

The `create` actions send `engine` and `scope` (for templates) or `template_name`,
`scope`, and `requested_by` (for runs) as authorization attributes. See
`docs/authz-action-matrix.md` for the full attribute definitions.

## MiniJinja Features

Templates use [MiniJinja](https://github.com/mitsuhiko/minijinja) syntax. Key
features available in export templates:

**Variable interpolation:** `{{ zone.name }}`, `{{ record.ttl }}`

**Filters:** `{{ zone.email | replace("@", ".") }}`,
`{{ record.ttl | default(value=zone.default_ttl) }}`

**Loops:**
```jinja
{% for record in records %}
{{ record.owner_name }} IN {{ record.type_name }} {{ record.rendered }}
{% endfor %}
```

**Conditionals:**
```jinja
{% if owner == zone.name %}{% set owner = "@" %}{% endif %}
```

**Variable assignment:** `{% set owner = record.owner_name %}`

**Comments:** `{# This is a comment #}`

All standard MiniJinja built-in filters are available. No custom filters or
functions are registered beyond the defaults.

## Examples

### Custom inventory template: network report

Create a template that lists all networks with their descriptions:

```bash
curl -X POST http://localhost:8080/api/v1/workflows/export-templates \
  -H 'Content-Type: application/json' \
  -d '{
    "name": "network-report",
    "description": "List all networks with descriptions",
    "engine": "minijinja",
    "scope": "inventory",
    "body": "# Network Report\n{% for network in networks %}{{ network.cidr }}\t{{ network.description }}\t(reserved: {{ network.reserved }})\n{% endfor %}"
  }'
```

Create and execute a run:

```bash
# Queue the run
curl -X POST http://localhost:8080/api/v1/workflows/export-runs \
  -H 'Content-Type: application/json' \
  -d '{
    "template_name": "network-report",
    "scope": "inventory"
  }'

# Execute the next pending task (returns the rendered output)
curl -X POST http://localhost:8080/api/v1/workflows/tasks/run-next
```

The response includes the rendered output in `workflow_result.rendered_output`.

### Rendering a forward zone file

```bash
curl -X POST http://localhost:8080/api/v1/workflows/export-runs \
  -H 'Content-Type: application/json' \
  -d '{
    "template_name": "bind-forward-zone",
    "scope": "forward_zone",
    "parameters": { "zone_name": "example.com" }
  }'

curl -X POST http://localhost:8080/api/v1/workflows/tasks/run-next
```
