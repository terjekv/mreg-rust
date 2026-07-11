CREATE INDEX IF NOT EXISTS idx_ip_addresses_host_id ON ip_addresses (host_id);
CREATE INDEX IF NOT EXISTS idx_records_owner_id ON records (owner_id);
CREATE INDEX IF NOT EXISTS idx_rrsets_anchor_id ON rrsets (anchor_id);

CREATE INDEX IF NOT EXISTS idx_host_contacts_hosts_contact_id
    ON host_contacts_hosts (contact_id);
CREATE INDEX IF NOT EXISTS idx_host_group_hosts_host_id
    ON host_group_hosts (host_id);
CREATE INDEX IF NOT EXISTS idx_host_group_parents_parent_id
    ON host_group_parents (parent_group_id);
CREATE INDEX IF NOT EXISTS idx_host_policy_role_atoms_atom_id
    ON host_policy_role_atoms (atom_id);
CREATE INDEX IF NOT EXISTS idx_host_policy_role_hosts_host_id
    ON host_policy_role_hosts (host_id);
CREATE INDEX IF NOT EXISTS idx_host_policy_role_labels_label_id
    ON host_policy_role_labels (label_id);
