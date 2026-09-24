DROP TRIGGER IF EXISTS reverse_zones_validate_name_kind ON reverse_zones;
DROP TRIGGER IF EXISTS forward_zones_validate_name_kind ON forward_zones;
DROP FUNCTION IF EXISTS validate_zone_name_kind_uniqueness();

DROP INDEX IF EXISTS rrsets_unique_owner_zone;
ALTER TABLE rrsets ADD CONSTRAINT rrsets_unique_owner UNIQUE (type_id, dns_class, owner_name);

DROP TRIGGER IF EXISTS network_excluded_ranges_reject_frozen_mutation ON network_excluded_ranges;
DROP FUNCTION IF EXISTS reject_frozen_excluded_range_mutation();
DROP TRIGGER IF EXISTS host_community_assignments_reject_frozen_mutation ON host_community_assignments;
DROP FUNCTION IF EXISTS reject_frozen_host_community_mutation();
DROP TRIGGER IF EXISTS communities_reject_frozen_mutation ON communities;
DROP FUNCTION IF EXISTS reject_frozen_community_mutation();
DROP TRIGGER IF EXISTS networks_reject_frozen_mutation ON networks;
DROP FUNCTION IF EXISTS reject_frozen_network_mutation();
DROP TRIGGER IF EXISTS attachment_community_assignments_reject_frozen_mutation ON attachment_community_assignments;
DROP TRIGGER IF EXISTS attachment_prefix_reservations_reject_frozen_mutation ON attachment_prefix_reservations;
DROP TRIGGER IF EXISTS attachment_dhcp_identifiers_reject_frozen_mutation ON attachment_dhcp_identifiers;
DROP TRIGGER IF EXISTS ip_addresses_reject_frozen_mutation ON ip_addresses;
DROP FUNCTION IF EXISTS reject_frozen_attachment_child_mutation();
DROP TRIGGER IF EXISTS host_attachments_reject_frozen_mutation ON host_attachments;
DROP FUNCTION IF EXISTS reject_frozen_host_attachment_mutation();

DROP TRIGGER IF EXISTS attachment_prefix_reservations_validate ON attachment_prefix_reservations;
DROP FUNCTION IF EXISTS validate_prefix_reservation();
DROP TRIGGER IF EXISTS host_community_assignments_validate_network ON host_community_assignments;
DROP FUNCTION IF EXISTS validate_host_community_network();
DROP TRIGGER IF EXISTS attachment_community_assignments_validate_network ON attachment_community_assignments;
DROP FUNCTION IF EXISTS validate_attachment_community_network();
DROP TRIGGER IF EXISTS network_excluded_ranges_validate ON network_excluded_ranges;
DROP FUNCTION IF EXISTS validate_excluded_network_range();
DROP TRIGGER IF EXISTS ip_addresses_validate_attachment ON ip_addresses;
DROP FUNCTION IF EXISTS validate_attachment_inventory();

DROP TABLE IF EXISTS managed_ip_records;

DROP INDEX IF EXISTS history_events_pending_delivery_idx;
ALTER TABLE history_events
    DROP COLUMN IF EXISTS delivery_error,
    DROP COLUMN IF EXISTS delivered_at,
    DROP COLUMN IF EXISTS delivery_lease_until,
    DROP COLUMN IF EXISTS delivery_lease_id,
    DROP COLUMN IF EXISTS delivery_available_at,
    DROP COLUMN IF EXISTS delivery_attempts;

ALTER TABLE ptr_overrides DROP CONSTRAINT IF EXISTS ptr_overrides_assignment_fkey;
ALTER TABLE ip_addresses DROP CONSTRAINT IF EXISTS ip_addresses_host_address_unique;

ALTER TABLE host_attachments DROP CONSTRAINT IF EXISTS host_attachments_network_id_fkey;
ALTER TABLE host_attachments
    ADD CONSTRAINT host_attachments_network_id_fkey
    FOREIGN KEY (network_id) REFERENCES networks(id) ON DELETE CASCADE;

ALTER TABLE hosts DROP CONSTRAINT IF EXISTS hosts_zone_id_fkey;
ALTER TABLE hosts
    ADD CONSTRAINT hosts_zone_id_fkey
    FOREIGN KEY (zone_id) REFERENCES forward_zones(id) ON DELETE SET NULL;

ALTER TABLE forward_zone_nameservers DROP CONSTRAINT IF EXISTS forward_zone_nameservers_nameserver_id_fkey;
ALTER TABLE forward_zone_nameservers
    ADD CONSTRAINT forward_zone_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE CASCADE;
ALTER TABLE reverse_zone_nameservers DROP CONSTRAINT IF EXISTS reverse_zone_nameservers_nameserver_id_fkey;
ALTER TABLE reverse_zone_nameservers
    ADD CONSTRAINT reverse_zone_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE CASCADE;
ALTER TABLE forward_zone_delegation_nameservers DROP CONSTRAINT IF EXISTS forward_zone_delegation_nameservers_nameserver_id_fkey;
ALTER TABLE forward_zone_delegation_nameservers
    ADD CONSTRAINT forward_zone_delegation_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE CASCADE;
ALTER TABLE reverse_zone_delegation_nameservers DROP CONSTRAINT IF EXISTS reverse_zone_delegation_nameservers_nameserver_id_fkey;
ALTER TABLE reverse_zone_delegation_nameservers
    ADD CONSTRAINT reverse_zone_delegation_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE CASCADE;

ALTER TABLE communities DROP CONSTRAINT IF EXISTS communities_network_name_unique;
ALTER TABLE communities ADD CONSTRAINT communities_policy_name_unique UNIQUE (policy_id, name);

DROP INDEX IF EXISTS host_attachments_identity_unique;
ALTER TABLE host_attachments ADD CONSTRAINT host_attachment_unique_with_mac UNIQUE (host_id, network_id, mac_address);
DROP INDEX IF EXISTS records_rrset_payload_unique;
ALTER TABLE records DROP CONSTRAINT IF EXISTS records_content_xor;
ALTER TABLE rrsets DROP CONSTRAINT IF EXISTS rrsets_ttl_nonnegative;
DROP INDEX IF EXISTS record_types_dns_type_unique;
ALTER TABLE record_types DROP CONSTRAINT IF EXISTS record_types_dns_type_assignable;
ALTER TABLE bacnet_ids DROP CONSTRAINT IF EXISTS bacnet_ids_object_instance;
ALTER TABLE networks DROP CONSTRAINT IF EXISTS networks_vlan_configurable;
ALTER TABLE networks DROP CONSTRAINT IF EXISTS networks_reserved_capacity;
ALTER TABLE networks DROP CONSTRAINT IF EXISTS networks_reserved_nonnegative;
ALTER TABLE hosts DROP CONSTRAINT IF EXISTS hosts_ttl_nonnegative;
ALTER TABLE nameservers DROP CONSTRAINT IF EXISTS nameservers_ttl_nonnegative;
ALTER TABLE reverse_zones DROP CONSTRAINT IF EXISTS reverse_zones_serial_u32;
ALTER TABLE reverse_zones DROP CONSTRAINT IF EXISTS reverse_zones_ttl_nonnegative;
ALTER TABLE reverse_zones DROP CONSTRAINT IF EXISTS reverse_zones_soa_time_nonnegative;
ALTER TABLE forward_zones DROP CONSTRAINT IF EXISTS forward_zones_serial_u32;
ALTER TABLE forward_zones DROP CONSTRAINT IF EXISTS forward_zones_ttl_nonnegative;
ALTER TABLE forward_zones DROP CONSTRAINT IF EXISTS forward_zones_soa_time_nonnegative;

ALTER TABLE reverse_zones DROP COLUMN soa_record_ttl;
ALTER TABLE forward_zones DROP COLUMN soa_record_ttl;
ALTER TABLE reverse_zones RENAME COLUMN negative_ttl TO soa_ttl;
ALTER TABLE forward_zones RENAME COLUMN negative_ttl TO soa_ttl;
