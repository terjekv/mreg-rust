-- Extensions
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Note: case-insensitive behavior is enforced by the application layer.
-- All domain types (DnsName, Hostname, LabelName, etc.) normalize to
-- lowercase on construction. Plain TEXT columns are used throughout.

-- Labels
CREATE TABLE labels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Host contacts
CREATE TABLE host_contacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL UNIQUE,
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Nameservers
CREATE TABLE nameservers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    ttl INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Forward zones
CREATE TABLE forward_zones (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    updated BOOLEAN NOT NULL DEFAULT TRUE,
    primary_ns TEXT NOT NULL,
    email TEXT NOT NULL,
    serial_no BIGINT NOT NULL DEFAULT 1,
    serial_no_updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    refresh INTEGER NOT NULL DEFAULT 10800,
    retry INTEGER NOT NULL DEFAULT 3600,
    expire INTEGER NOT NULL DEFAULT 1814400,
    soa_ttl INTEGER NOT NULL DEFAULT 43200,
    default_ttl INTEGER NOT NULL DEFAULT 43200,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Reverse zones
CREATE TABLE reverse_zones (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    network CIDR,
    updated BOOLEAN NOT NULL DEFAULT TRUE,
    primary_ns TEXT NOT NULL,
    email TEXT NOT NULL,
    serial_no BIGINT NOT NULL DEFAULT 1,
    serial_no_updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    refresh INTEGER NOT NULL DEFAULT 10800,
    retry INTEGER NOT NULL DEFAULT 3600,
    expire INTEGER NOT NULL DEFAULT 1814400,
    soa_ttl INTEGER NOT NULL DEFAULT 43200,
    default_ttl INTEGER NOT NULL DEFAULT 43200,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Zone nameserver junctions
CREATE TABLE forward_zone_nameservers (
    zone_id UUID NOT NULL REFERENCES forward_zones(id) ON DELETE CASCADE,
    nameserver_id UUID NOT NULL REFERENCES nameservers(id) ON DELETE CASCADE,
    PRIMARY KEY (zone_id, nameserver_id)
);

CREATE TABLE reverse_zone_nameservers (
    zone_id UUID NOT NULL REFERENCES reverse_zones(id) ON DELETE CASCADE,
    nameserver_id UUID NOT NULL REFERENCES nameservers(id) ON DELETE CASCADE,
    PRIMARY KEY (zone_id, nameserver_id)
);

-- Zone delegations
CREATE TABLE forward_zone_delegations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    zone_id UUID NOT NULL REFERENCES forward_zones(id) ON DELETE CASCADE,
    name TEXT NOT NULL UNIQUE,
    comment TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE reverse_zone_delegations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    zone_id UUID NOT NULL REFERENCES reverse_zones(id) ON DELETE CASCADE,
    name TEXT NOT NULL UNIQUE,
    comment TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE forward_zone_delegation_nameservers (
    delegation_id UUID NOT NULL REFERENCES forward_zone_delegations(id) ON DELETE CASCADE,
    nameserver_id UUID NOT NULL REFERENCES nameservers(id) ON DELETE CASCADE,
    PRIMARY KEY (delegation_id, nameserver_id)
);

CREATE TABLE reverse_zone_delegation_nameservers (
    delegation_id UUID NOT NULL REFERENCES reverse_zone_delegations(id) ON DELETE CASCADE,
    nameserver_id UUID NOT NULL REFERENCES nameservers(id) ON DELETE CASCADE,
    PRIMARY KEY (delegation_id, nameserver_id)
);

-- Network policies
CREATE TABLE network_policies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    community_template_pattern TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE network_policy_attributes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE network_policy_attribute_values (
    policy_id UUID NOT NULL REFERENCES network_policies(id) ON DELETE CASCADE,
    attribute_id UUID NOT NULL REFERENCES network_policy_attributes(id) ON DELETE CASCADE,
    value BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (policy_id, attribute_id)
);

-- Networks
CREATE TABLE networks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    network CIDR NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    vlan INTEGER,
    dns_delegated BOOLEAN NOT NULL DEFAULT FALSE,
    category TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',
    frozen BOOLEAN NOT NULL DEFAULT FALSE,
    reserved INTEGER NOT NULL DEFAULT 3,
    max_communities INTEGER,
    policy_id UUID REFERENCES network_policies(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE network_excluded_ranges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    network_id UUID NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    start_ip INET NOT NULL,
    end_ip INET NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT network_excluded_ranges_unique_span UNIQUE (network_id, start_ip, end_ip)
);

-- Communities
CREATE TABLE communities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_id UUID NOT NULL REFERENCES network_policies(id) ON DELETE CASCADE,
    network_id UUID NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT communities_policy_name_unique UNIQUE (policy_id, name)
);

-- Hosts
CREATE TABLE hosts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    zone_id UUID REFERENCES forward_zones(id) ON DELETE SET NULL,
    ttl INTEGER,
    comment TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE host_contacts_hosts (
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    contact_id UUID NOT NULL REFERENCES host_contacts(id) ON DELETE CASCADE,
    PRIMARY KEY (host_id, contact_id)
);

-- Host groups
CREATE TABLE host_groups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE host_group_hosts (
    host_group_id UUID NOT NULL REFERENCES host_groups(id) ON DELETE CASCADE,
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    PRIMARY KEY (host_group_id, host_id)
);

CREATE TABLE host_group_parents (
    host_group_id UUID NOT NULL REFERENCES host_groups(id) ON DELETE CASCADE,
    parent_group_id UUID NOT NULL REFERENCES host_groups(id) ON DELETE CASCADE,
    PRIMARY KEY (host_group_id, parent_group_id),
    CONSTRAINT host_group_parents_not_self CHECK (host_group_id <> parent_group_id)
);

CREATE TABLE host_group_owner_groups (
    host_group_id UUID NOT NULL REFERENCES host_groups(id) ON DELETE CASCADE,
    owner_group TEXT NOT NULL,
    PRIMARY KEY (host_group_id, owner_group)
);

-- BACnet
CREATE TABLE bacnet_ids (
    id INTEGER PRIMARY KEY,
    host_id UUID NOT NULL UNIQUE REFERENCES hosts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Host attachments
CREATE TABLE host_attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    network_id UUID NOT NULL REFERENCES networks(id) ON DELETE CASCADE,
    mac_address TEXT,
    comment TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT host_attachment_unique_with_mac UNIQUE (host_id, network_id, mac_address)
);

CREATE INDEX host_attachments_host_idx ON host_attachments(host_id);
CREATE INDEX host_attachments_network_idx ON host_attachments(network_id);

-- IP addresses
CREATE TABLE ip_addresses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    attachment_id UUID NOT NULL REFERENCES host_attachments(id) ON DELETE CASCADE,
    address INET NOT NULL UNIQUE,
    family SMALLINT NOT NULL,
    mac_address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT ip_addresses_family_check CHECK (family IN (4, 6))
);

CREATE INDEX ip_addresses_attachment_idx ON ip_addresses(attachment_id);

-- PTR overrides
CREATE TABLE ptr_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    address INET NOT NULL UNIQUE,
    target_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Host community assignments (legacy; prefer attachment_community_assignments)
CREATE TABLE host_community_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    ip_address_id UUID NOT NULL REFERENCES ip_addresses(id) ON DELETE CASCADE,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT host_community_assignment_unique UNIQUE (host_id, ip_address_id, community_id)
);

-- Attachment DHCP identifiers
CREATE TABLE attachment_dhcp_identifiers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attachment_id UUID NOT NULL REFERENCES host_attachments(id) ON DELETE CASCADE,
    family SMALLINT NOT NULL,
    kind TEXT NOT NULL,
    value TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT attachment_dhcp_identifiers_family_check CHECK (family IN (4, 6)),
    CONSTRAINT attachment_dhcp_identifiers_unique UNIQUE (attachment_id, family, kind, value)
);

CREATE INDEX attachment_dhcp_identifiers_attachment_idx ON attachment_dhcp_identifiers(attachment_id);

-- Attachment prefix reservations
CREATE TABLE attachment_prefix_reservations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attachment_id UUID NOT NULL REFERENCES host_attachments(id) ON DELETE CASCADE,
    prefix CIDR NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX attachment_prefix_reservations_attachment_idx ON attachment_prefix_reservations(attachment_id);

-- Attachment community assignments
CREATE TABLE attachment_community_assignments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attachment_id UUID NOT NULL REFERENCES host_attachments(id) ON DELETE CASCADE,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT attachment_community_assignment_unique UNIQUE (attachment_id, community_id)
);

CREATE INDEX attachment_community_assignments_attachment_idx ON attachment_community_assignments(attachment_id);

-- Record types
CREATE TABLE record_types (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    dns_type INTEGER,
    owner_kind TEXT NOT NULL,
    cardinality TEXT NOT NULL,
    validation_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    rendering_schema JSONB NOT NULL DEFAULT '{}'::jsonb,
    behavior_flags JSONB NOT NULL DEFAULT '{}'::jsonb,
    built_in BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT record_types_cardinality_check CHECK (cardinality IN ('single', 'multiple'))
);

-- RRsets
CREATE TABLE rrsets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type_id UUID NOT NULL REFERENCES record_types(id) ON DELETE RESTRICT,
    dns_class TEXT NOT NULL DEFAULT 'IN',
    owner_name TEXT NOT NULL,
    anchor_kind TEXT,
    anchor_id UUID,
    anchor_name TEXT,
    zone_kind TEXT,
    zone_id UUID,
    ttl INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT rrsets_dns_class_check CHECK (dns_class IN ('IN')),
    CONSTRAINT rrsets_anchor_pair_check CHECK (
        (anchor_kind IS NULL AND anchor_id IS NULL)
        OR (anchor_kind IS NOT NULL AND anchor_id IS NOT NULL)
    ),
    CONSTRAINT rrsets_unique_owner UNIQUE (type_id, dns_class, owner_name)
);

-- Records
CREATE TABLE records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type_id UUID NOT NULL REFERENCES record_types(id) ON DELETE RESTRICT,
    owner_kind TEXT,
    owner_id UUID,
    owner_name TEXT,
    zone_kind TEXT,
    zone_id UUID,
    ttl INTEGER,
    rendered TEXT,
    rrset_id UUID NOT NULL REFERENCES rrsets(id) ON DELETE CASCADE,
    data JSONB NOT NULL,
    raw_rdata BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Host policy
CREATE TABLE host_policy_atoms (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE host_policy_roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE host_policy_role_atoms (
    role_id UUID NOT NULL REFERENCES host_policy_roles(id) ON DELETE CASCADE,
    atom_id UUID NOT NULL REFERENCES host_policy_atoms(id) ON DELETE RESTRICT,
    PRIMARY KEY (role_id, atom_id)
);

CREATE TABLE host_policy_role_hosts (
    role_id UUID NOT NULL REFERENCES host_policy_roles(id) ON DELETE CASCADE,
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, host_id)
);

CREATE TABLE host_policy_role_labels (
    role_id UUID NOT NULL REFERENCES host_policy_roles(id) ON DELETE CASCADE,
    label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, label_id)
);

-- Tasks
CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    idempotency_key TEXT UNIQUE,
    requested_by TEXT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    progress JSONB NOT NULL DEFAULT '{}'::jsonb,
    result JSONB,
    error_summary TEXT,
    error_details JSONB,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tasks_status_check CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled'))
);

-- Imports
CREATE TABLE imports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID UNIQUE REFERENCES tasks(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    requested_by TEXT,
    batch JSONB NOT NULL,
    normalized_batch JSONB,
    validation_report JSONB,
    commit_summary JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT imports_status_check CHECK (status IN ('queued', 'validating', 'ready', 'committing', 'succeeded', 'failed', 'cancelled'))
);

-- Export templates
CREATE TABLE export_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    engine TEXT NOT NULL,
    scope TEXT NOT NULL,
    body TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    built_in BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Export runs
CREATE TABLE export_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID UNIQUE REFERENCES tasks(id) ON DELETE SET NULL,
    template_id UUID REFERENCES export_templates(id) ON DELETE SET NULL,
    requested_by TEXT,
    scope TEXT NOT NULL,
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL,
    rendered_output TEXT,
    artifact_metadata JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT export_runs_status_check CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled'))
);

-- Authentication session revocation
CREATE TABLE revoked_tokens (
    token_fingerprint TEXT PRIMARY KEY,
    principal_id TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX revoked_tokens_principal_id_idx ON revoked_tokens (principal_id);
CREATE INDEX revoked_tokens_expires_at_idx ON revoked_tokens (expires_at);

CREATE TABLE principal_token_revocations (
    principal_id TEXT PRIMARY KEY,
    revoked_before TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- History / audit
CREATE TABLE history_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor TEXT NOT NULL,
    resource_kind TEXT NOT NULL,
    resource_id UUID,
    resource_name TEXT NOT NULL,
    action TEXT NOT NULL,
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes: trigram (cast to text with default collation for LIKE compatibility)
CREATE INDEX idx_labels_name_trgm ON labels USING GIN (name gin_trgm_ops);
CREATE INDEX idx_nameservers_name_trgm ON nameservers USING GIN (name gin_trgm_ops);
CREATE INDEX idx_forward_zones_name_trgm ON forward_zones USING GIN (name gin_trgm_ops);
CREATE INDEX idx_reverse_zones_name_trgm ON reverse_zones USING GIN (name gin_trgm_ops);
CREATE INDEX idx_hosts_name_trgm ON hosts USING GIN (name gin_trgm_ops);
CREATE INDEX idx_host_groups_name_trgm ON host_groups USING GIN (name gin_trgm_ops);
CREATE INDEX idx_rrsets_owner_name_trgm ON rrsets USING GIN (owner_name gin_trgm_ops);

-- Indexes: performance
CREATE INDEX idx_networks_network ON networks (network);
CREATE INDEX idx_ip_addresses_address ON ip_addresses (address);
CREATE INDEX idx_records_type_owner ON records (type_id, owner_kind, owner_id);
CREATE INDEX idx_records_data_gin ON records USING GIN (data);
CREATE INDEX idx_records_rrset ON records (rrset_id);
CREATE INDEX idx_rrsets_type_owner ON rrsets (type_id, owner_name);
CREATE INDEX idx_record_types_validation_schema_gin ON record_types USING GIN (validation_schema);
CREATE INDEX idx_tasks_status_available_at ON tasks (status, available_at);
CREATE INDEX idx_imports_status ON imports (status);
CREATE INDEX idx_export_runs_status ON export_runs (status);
CREATE INDEX idx_history_events_resource ON history_events (resource_kind, resource_id, created_at DESC);
CREATE INDEX idx_host_policy_atoms_name ON host_policy_atoms (name);
CREATE INDEX idx_host_policy_roles_name ON host_policy_roles (name);
CREATE INDEX idx_hosts_zone_id ON hosts (zone_id);
CREATE INDEX idx_rrsets_zone_id ON rrsets (zone_id);
CREATE INDEX idx_records_zone_id ON records (zone_id);
