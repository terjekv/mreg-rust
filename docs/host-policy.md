# Host Policy

Host policy provides a hierarchical tagging system for applying operational policies to hosts. It consists of **atoms** (individual policy properties) and **roles** (named collections of atoms assigned to hosts).

## Concepts

### Atoms

An atom is a named policy property — a single behavioral flag or configuration directive. Examples:

- `autoconfigure` — host should be auto-configured by management systems
- `no-icmp` — host should not respond to ICMP
- `dhcp-enabled` — host participates in DHCP
- `monitored` — host is under active monitoring

Atoms are reusable building blocks. They have a name and an optional description.

### Roles

A role is a named collection of atoms that can be assigned to hosts. Roles represent a complete policy profile. Examples:

- `standard-server` — contains atoms `autoconfigure`, `monitored`, `dhcp-enabled`
- `restricted-host` — contains atoms `no-icmp`, `monitored`
- `lab-equipment` — contains atoms `autoconfigure`, `dhcp-enabled`

Roles can also reference labels, creating a link between policy roles and the label/tagging system.

### Membership

- Roles **contain** atoms (many-to-many)
- Roles **are assigned to** hosts (many-to-many)
- Roles **reference** labels (many-to-many)

Deleting an atom that is referenced by any role is rejected (RESTRICT). Deleting a role cascades to remove all its atom, host, and label associations.

## API

### Atom management

```
POST   /api/v1/policy/host/atoms              Create atom
GET    /api/v1/policy/host/atoms              List atoms (paginated)
GET    /api/v1/policy/host/atoms/{name}       Get atom details
PATCH  /api/v1/policy/host/atoms/{name}       Update atom description
DELETE /api/v1/policy/host/atoms/{name}       Delete atom (409 if in use)
```

**Create atom:**
```json
POST /api/v1/policy/host/atoms
{
  "name": "autoconfigure",
  "description": "Host should be auto-configured"
}
```

### Role management

```
POST   /api/v1/policy/host/roles              Create role
GET    /api/v1/policy/host/roles              List roles (paginated)
GET    /api/v1/policy/host/roles/{name}       Get role (includes atoms, hosts, labels)
PATCH  /api/v1/policy/host/roles/{name}       Update role description
DELETE /api/v1/policy/host/roles/{name}       Delete role (cascades)
```

**Create role:**
```json
POST /api/v1/policy/host/roles
{
  "name": "standard-server",
  "description": "Standard server policy"
}
```

**Get role response:**
```json
{
  "id": "...",
  "name": "standard-server",
  "description": "Standard server policy",
  "atoms": ["autoconfigure", "monitored", "dhcp-enabled"],
  "hosts": ["web.example.org", "db.example.org"],
  "labels": ["production"],
  "created_at": "...",
  "updated_at": "..."
}
```

### Membership management

```
POST   /api/v1/policy/host/roles/{role}/atoms/{atom}     Add atom to role
DELETE /api/v1/policy/host/roles/{role}/atoms/{atom}     Remove atom from role

POST   /api/v1/policy/host/roles/{role}/hosts/{host}     Assign host to role
DELETE /api/v1/policy/host/roles/{role}/hosts/{host}     Remove host from role

POST   /api/v1/policy/host/roles/{role}/labels/{label}   Link label to role
DELETE /api/v1/policy/host/roles/{role}/labels/{label}   Unlink label from role
```

## Database schema

```
host_policy_atoms         — id, name (unique), description, timestamps
host_policy_roles         — id, name (unique), description, timestamps
host_policy_role_atoms    — role_id → atom_id (CASCADE/RESTRICT)
host_policy_role_hosts    — role_id → host_id (CASCADE/CASCADE)
host_policy_role_labels   — role_id → label_id (CASCADE/CASCADE)
```

## Relationship to labels

Labels and host policy roles are complementary:

- **Labels** are simple, flat tags attached to hosts for categorization and filtering
- **Roles** are structured policy containers that group atoms and can reference labels
- A role can include labels, creating a bridge: "all hosts with role X should also have label Y"

## Relationship to the old mreg

The host policy system in mreg-rust maps directly to the old mreg's `hostpolicy` module:

| Old mreg | mreg-rust |
|----------|-----------|
| `POST /api/v1/hostpolicy/atoms/` | `POST /api/v1/policy/host/atoms` |
| `POST /api/v1/hostpolicy/roles/` | `POST /api/v1/policy/host/roles` |
| `PATCH /api/v1/hostpolicy/roles/{name}/hosts/` | `POST /api/v1/policy/host/roles/{name}/hosts/{host}` |

The key difference is URL structure (kebab-case paths, RESTful sub-resources) and that membership operations use individual POST/DELETE rather than PATCH with lists.
