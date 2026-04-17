// @generated automatically by Diesel CLI.

diesel::table! {
    attachment_community_assignments (id) {
        id -> Uuid,
        attachment_id -> Uuid,
        community_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    attachment_dhcp_identifiers (id) {
        id -> Uuid,
        attachment_id -> Uuid,
        family -> Int2,
        kind -> Text,
        value -> Text,
        priority -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    attachment_prefix_reservations (id) {
        id -> Uuid,
        attachment_id -> Uuid,
        prefix -> Cidr,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    bacnet_ids (id) {
        id -> Int4,
        host_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    communities (id) {
        id -> Uuid,
        policy_id -> Uuid,
        network_id -> Uuid,
        name -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    export_runs (id) {
        id -> Uuid,
        task_id -> Nullable<Uuid>,
        template_id -> Nullable<Uuid>,
        requested_by -> Nullable<Text>,
        scope -> Text,
        parameters -> Jsonb,
        status -> Text,
        rendered_output -> Nullable<Text>,
        artifact_metadata -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    export_templates (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        engine -> Text,
        scope -> Text,
        body -> Text,
        metadata -> Jsonb,
        built_in -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    forward_zone_delegation_nameservers (delegation_id, nameserver_id) {
        delegation_id -> Uuid,
        nameserver_id -> Uuid,
    }
}

diesel::table! {
    forward_zone_delegations (id) {
        id -> Uuid,
        zone_id -> Uuid,
        name -> Text,
        comment -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    forward_zone_nameservers (zone_id, nameserver_id) {
        zone_id -> Uuid,
        nameserver_id -> Uuid,
    }
}

diesel::table! {
    forward_zones (id) {
        id -> Uuid,
        name -> Text,
        updated -> Bool,
        primary_ns -> Text,
        email -> Text,
        serial_no -> Int8,
        serial_no_updated_at -> Timestamptz,
        refresh -> Int4,
        retry -> Int4,
        expire -> Int4,
        soa_ttl -> Int4,
        default_ttl -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    history_events (id) {
        id -> Uuid,
        actor -> Text,
        resource_kind -> Text,
        resource_id -> Nullable<Uuid>,
        resource_name -> Text,
        action -> Text,
        data -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    host_attachments (id) {
        id -> Uuid,
        host_id -> Uuid,
        network_id -> Uuid,
        mac_address -> Nullable<Text>,
        comment -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    host_community_assignments (id) {
        id -> Uuid,
        host_id -> Uuid,
        ip_address_id -> Uuid,
        community_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    host_contacts (id) {
        id -> Uuid,
        email -> Text,
        display_name -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    host_contacts_hosts (host_id, contact_id) {
        host_id -> Uuid,
        contact_id -> Uuid,
    }
}

diesel::table! {
    host_group_hosts (host_group_id, host_id) {
        host_group_id -> Uuid,
        host_id -> Uuid,
    }
}

diesel::table! {
    host_group_owner_groups (host_group_id, owner_group) {
        host_group_id -> Uuid,
        owner_group -> Text,
    }
}

diesel::table! {
    host_group_parents (host_group_id, parent_group_id) {
        host_group_id -> Uuid,
        parent_group_id -> Uuid,
    }
}

diesel::table! {
    host_groups (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    host_policy_atoms (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    host_policy_role_atoms (role_id, atom_id) {
        role_id -> Uuid,
        atom_id -> Uuid,
    }
}

diesel::table! {
    host_policy_role_hosts (role_id, host_id) {
        role_id -> Uuid,
        host_id -> Uuid,
    }
}

diesel::table! {
    host_policy_role_labels (role_id, label_id) {
        role_id -> Uuid,
        label_id -> Uuid,
    }
}

diesel::table! {
    host_policy_roles (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    hosts (id) {
        id -> Uuid,
        name -> Text,
        zone_id -> Nullable<Uuid>,
        ttl -> Nullable<Int4>,
        comment -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    imports (id) {
        id -> Uuid,
        task_id -> Nullable<Uuid>,
        status -> Text,
        requested_by -> Nullable<Text>,
        batch -> Jsonb,
        normalized_batch -> Nullable<Jsonb>,
        validation_report -> Nullable<Jsonb>,
        commit_summary -> Nullable<Jsonb>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    ip_addresses (id) {
        id -> Uuid,
        host_id -> Uuid,
        attachment_id -> Uuid,
        address -> Inet,
        family -> Int2,
        mac_address -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    labels (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    nameservers (id) {
        id -> Uuid,
        name -> Text,
        ttl -> Nullable<Int4>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    network_excluded_ranges (id) {
        id -> Uuid,
        network_id -> Uuid,
        start_ip -> Inet,
        end_ip -> Inet,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    network_policies (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        community_template_pattern -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    network_policy_attribute_values (policy_id, attribute_id) {
        policy_id -> Uuid,
        attribute_id -> Uuid,
        value -> Bool,
    }
}

diesel::table! {
    network_policy_attributes (id) {
        id -> Uuid,
        name -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    networks (id) {
        id -> Uuid,
        network -> Cidr,
        description -> Text,
        vlan -> Nullable<Int4>,
        dns_delegated -> Bool,
        category -> Text,
        location -> Text,
        frozen -> Bool,
        reserved -> Int4,
        max_communities -> Nullable<Int4>,
        policy_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    principal_token_revocations (principal_id) {
        principal_id -> Text,
        revoked_before -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    ptr_overrides (id) {
        id -> Uuid,
        host_id -> Uuid,
        address -> Inet,
        target_name -> Nullable<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    record_types (id) {
        id -> Uuid,
        name -> Text,
        dns_type -> Nullable<Int4>,
        owner_kind -> Text,
        cardinality -> Text,
        validation_schema -> Jsonb,
        rendering_schema -> Jsonb,
        behavior_flags -> Jsonb,
        built_in -> Bool,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    records (id) {
        id -> Uuid,
        type_id -> Uuid,
        owner_kind -> Nullable<Text>,
        owner_id -> Nullable<Uuid>,
        owner_name -> Nullable<Text>,
        zone_kind -> Nullable<Text>,
        zone_id -> Nullable<Uuid>,
        ttl -> Nullable<Int4>,
        rendered -> Nullable<Text>,
        rrset_id -> Uuid,
        data -> Jsonb,
        raw_rdata -> Nullable<Bytea>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    revoked_tokens (token_fingerprint) {
        token_fingerprint -> Text,
        principal_id -> Text,
        revoked_at -> Timestamptz,
        expires_at -> Timestamptz,
    }
}

diesel::table! {
    reverse_zone_delegation_nameservers (delegation_id, nameserver_id) {
        delegation_id -> Uuid,
        nameserver_id -> Uuid,
    }
}

diesel::table! {
    reverse_zone_delegations (id) {
        id -> Uuid,
        zone_id -> Uuid,
        name -> Text,
        comment -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    reverse_zone_nameservers (zone_id, nameserver_id) {
        zone_id -> Uuid,
        nameserver_id -> Uuid,
    }
}

diesel::table! {
    reverse_zones (id) {
        id -> Uuid,
        name -> Text,
        network -> Nullable<Cidr>,
        updated -> Bool,
        primary_ns -> Text,
        email -> Text,
        serial_no -> Int8,
        serial_no_updated_at -> Timestamptz,
        refresh -> Int4,
        retry -> Int4,
        expire -> Int4,
        soa_ttl -> Int4,
        default_ttl -> Int4,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    rrsets (id) {
        id -> Uuid,
        type_id -> Uuid,
        dns_class -> Text,
        owner_name -> Text,
        anchor_kind -> Nullable<Text>,
        anchor_id -> Nullable<Uuid>,
        anchor_name -> Nullable<Text>,
        zone_kind -> Nullable<Text>,
        zone_id -> Nullable<Uuid>,
        ttl -> Nullable<Int4>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    tasks (id) {
        id -> Uuid,
        kind -> Text,
        status -> Text,
        idempotency_key -> Nullable<Text>,
        requested_by -> Nullable<Text>,
        payload -> Jsonb,
        progress -> Jsonb,
        result -> Nullable<Jsonb>,
        error_summary -> Nullable<Text>,
        error_details -> Nullable<Jsonb>,
        attempts -> Int4,
        max_attempts -> Int4,
        available_at -> Timestamptz,
        started_at -> Nullable<Timestamptz>,
        finished_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::joinable!(attachment_community_assignments -> communities (community_id));
diesel::joinable!(attachment_community_assignments -> host_attachments (attachment_id));
diesel::joinable!(attachment_dhcp_identifiers -> host_attachments (attachment_id));
diesel::joinable!(attachment_prefix_reservations -> host_attachments (attachment_id));
diesel::joinable!(bacnet_ids -> hosts (host_id));
diesel::joinable!(communities -> network_policies (policy_id));
diesel::joinable!(communities -> networks (network_id));
diesel::joinable!(export_runs -> export_templates (template_id));
diesel::joinable!(export_runs -> tasks (task_id));
diesel::joinable!(forward_zone_delegation_nameservers -> forward_zone_delegations (delegation_id));
diesel::joinable!(forward_zone_delegation_nameservers -> nameservers (nameserver_id));
diesel::joinable!(forward_zone_delegations -> forward_zones (zone_id));
diesel::joinable!(forward_zone_nameservers -> forward_zones (zone_id));
diesel::joinable!(forward_zone_nameservers -> nameservers (nameserver_id));
diesel::joinable!(host_community_assignments -> communities (community_id));
diesel::joinable!(host_community_assignments -> hosts (host_id));
diesel::joinable!(host_community_assignments -> ip_addresses (ip_address_id));
diesel::joinable!(host_attachments -> hosts (host_id));
diesel::joinable!(host_attachments -> networks (network_id));
diesel::joinable!(host_contacts_hosts -> host_contacts (contact_id));
diesel::joinable!(host_contacts_hosts -> hosts (host_id));
diesel::joinable!(host_group_hosts -> host_groups (host_group_id));
diesel::joinable!(host_group_hosts -> hosts (host_id));
diesel::joinable!(host_group_owner_groups -> host_groups (host_group_id));
diesel::joinable!(host_policy_role_atoms -> host_policy_atoms (atom_id));
diesel::joinable!(host_policy_role_atoms -> host_policy_roles (role_id));
diesel::joinable!(host_policy_role_hosts -> host_policy_roles (role_id));
diesel::joinable!(host_policy_role_hosts -> hosts (host_id));
diesel::joinable!(host_policy_role_labels -> host_policy_roles (role_id));
diesel::joinable!(host_policy_role_labels -> labels (label_id));
diesel::joinable!(hosts -> forward_zones (zone_id));
diesel::joinable!(imports -> tasks (task_id));
diesel::joinable!(ip_addresses -> hosts (host_id));
diesel::joinable!(ip_addresses -> host_attachments (attachment_id));
diesel::joinable!(network_excluded_ranges -> networks (network_id));
diesel::joinable!(network_policy_attribute_values -> network_policies (policy_id));
diesel::joinable!(network_policy_attribute_values -> network_policy_attributes (attribute_id));
diesel::joinable!(networks -> network_policies (policy_id));
diesel::joinable!(ptr_overrides -> hosts (host_id));
diesel::joinable!(records -> record_types (type_id));
diesel::joinable!(records -> rrsets (rrset_id));
diesel::joinable!(reverse_zone_delegation_nameservers -> nameservers (nameserver_id));
diesel::joinable!(reverse_zone_delegation_nameservers -> reverse_zone_delegations (delegation_id));
diesel::joinable!(reverse_zone_delegations -> reverse_zones (zone_id));
diesel::joinable!(reverse_zone_nameservers -> nameservers (nameserver_id));
diesel::joinable!(reverse_zone_nameservers -> reverse_zones (zone_id));
diesel::joinable!(rrsets -> record_types (type_id));

diesel::allow_tables_to_appear_in_same_query!(
    bacnet_ids,
    attachment_community_assignments,
    attachment_dhcp_identifiers,
    attachment_prefix_reservations,
    communities,
    export_runs,
    export_templates,
    forward_zone_delegation_nameservers,
    forward_zone_delegations,
    forward_zone_nameservers,
    forward_zones,
    history_events,
    host_attachments,
    host_community_assignments,
    host_contacts,
    host_contacts_hosts,
    host_group_hosts,
    host_group_owner_groups,
    host_group_parents,
    host_groups,
    host_policy_atoms,
    host_policy_role_atoms,
    host_policy_role_hosts,
    host_policy_role_labels,
    host_policy_roles,
    hosts,
    imports,
    ip_addresses,
    labels,
    nameservers,
    network_excluded_ranges,
    network_policies,
    network_policy_attribute_values,
    network_policy_attributes,
    networks,
    principal_token_revocations,
    ptr_overrides,
    record_types,
    records,
    revoked_tokens,
    reverse_zone_delegation_nameservers,
    reverse_zone_delegations,
    reverse_zone_nameservers,
    reverse_zones,
    rrsets,
    tasks,
);
