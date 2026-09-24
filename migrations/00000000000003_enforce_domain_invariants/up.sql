-- This migration intentionally tightens persisted domain invariants. Invalid
-- rows must be corrected before retrying the migration unless explicitly
-- normalized below.

ALTER TABLE forward_zones RENAME COLUMN soa_ttl TO negative_ttl;
ALTER TABLE reverse_zones RENAME COLUMN soa_ttl TO negative_ttl;
ALTER TABLE forward_zones ADD COLUMN soa_record_ttl INTEGER NOT NULL DEFAULT 43200;
ALTER TABLE reverse_zones ADD COLUMN soa_record_ttl INTEGER NOT NULL DEFAULT 43200;

-- Old YYYYMMDDNNNN serials exceeded the DNS 32-bit field. Preserve their low
-- 32 bits and require a full secondary refresh after upgrading.
UPDATE forward_zones SET serial_no = MOD(serial_no, 4294967296) WHERE serial_no NOT BETWEEN 0 AND 4294967295;
UPDATE reverse_zones SET serial_no = MOD(serial_no, 4294967296) WHERE serial_no NOT BETWEEN 0 AND 4294967295;

ALTER TABLE forward_zones
    ADD CONSTRAINT forward_zones_serial_u32 CHECK (serial_no BETWEEN 0 AND 4294967295),
    ADD CONSTRAINT forward_zones_ttl_nonnegative CHECK (soa_record_ttl >= 0 AND negative_ttl >= 0 AND default_ttl >= 0),
    ADD CONSTRAINT forward_zones_soa_time_nonnegative CHECK (refresh >= 0 AND retry >= 0 AND expire >= 0);
ALTER TABLE reverse_zones
    ADD CONSTRAINT reverse_zones_serial_u32 CHECK (serial_no BETWEEN 0 AND 4294967295),
    ADD CONSTRAINT reverse_zones_ttl_nonnegative CHECK (soa_record_ttl >= 0 AND negative_ttl >= 0 AND default_ttl >= 0),
    ADD CONSTRAINT reverse_zones_soa_time_nonnegative CHECK (refresh >= 0 AND retry >= 0 AND expire >= 0);

ALTER TABLE nameservers ADD CONSTRAINT nameservers_ttl_nonnegative CHECK (ttl IS NULL OR ttl >= 0);
ALTER TABLE hosts ADD CONSTRAINT hosts_ttl_nonnegative CHECK (ttl IS NULL OR ttl >= 0);
ALTER TABLE networks
    ADD CONSTRAINT networks_vlan_configurable CHECK (vlan IS NULL OR vlan BETWEEN 1 AND 4094),
    ADD CONSTRAINT networks_reserved_nonnegative CHECK (reserved >= 0),
    ADD CONSTRAINT networks_reserved_capacity CHECK (
        reserved::numeric <= CASE
            WHEN family(network) = 4 AND masklen(network) <= 30
                THEN power(2::numeric, 32 - masklen(network)) - 2
            ELSE power(2::numeric, CASE WHEN family(network) = 4 THEN 32 ELSE 128 END - masklen(network)) - 1
        END
    );
ALTER TABLE bacnet_ids ADD CONSTRAINT bacnet_ids_object_instance CHECK (id BETWEEN 0 AND 4194302);

-- The audit row and its delivery state are committed atomically, implementing
-- a transactional outbox without a second source of event payloads.
ALTER TABLE history_events
    ADD COLUMN delivery_attempts INTEGER NOT NULL DEFAULT 0 CHECK (delivery_attempts >= 0),
    ADD COLUMN delivery_available_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD COLUMN delivery_lease_id UUID,
    ADD COLUMN delivery_lease_until TIMESTAMPTZ,
    ADD COLUMN delivered_at TIMESTAMPTZ,
    ADD COLUMN delivery_error TEXT;
CREATE INDEX history_events_pending_delivery_idx
    ON history_events (delivery_available_at, created_at)
    WHERE delivered_at IS NULL;

ALTER TABLE record_types
    ADD CONSTRAINT record_types_dns_type_assignable CHECK (dns_type IS NULL OR dns_type BETWEEN 1 AND 65534);
CREATE UNIQUE INDEX record_types_dns_type_unique ON record_types (dns_type) WHERE dns_type IS NOT NULL;

ALTER TABLE rrsets ADD CONSTRAINT rrsets_ttl_nonnegative CHECK (ttl IS NULL OR ttl >= 0);
ALTER TABLE records
    ADD CONSTRAINT records_content_xor CHECK (
        (raw_rdata IS NULL AND data <> 'null'::jsonb)
        OR (raw_rdata IS NOT NULL AND data = 'null'::jsonb)
    );
CREATE UNIQUE INDEX records_rrset_payload_unique
    ON records (rrset_id, data, raw_rdata) NULLS NOT DISTINCT;

-- RRset identity is zone-local. At a zone cut the parent delegation and the
-- child apex legitimately have separate NS RRsets with the same owner/type.
ALTER TABLE rrsets DROP CONSTRAINT rrsets_unique_owner;
CREATE UNIQUE INDEX rrsets_unique_owner_zone
    ON rrsets (type_id, dns_class, owner_name, zone_id) NULLS NOT DISTINCT;

-- Record ownership stores a zone UUID without a table discriminator, so a
-- name cannot simultaneously identify a forward and reverse zone.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM forward_zones f JOIN reverse_zones r ON r.name = f.name
    ) THEN
        RAISE EXCEPTION 'forward and reverse zone names overlap';
    END IF;
END;
$$;

CREATE FUNCTION validate_zone_name_kind_uniqueness() RETURNS trigger AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtext(NEW.name::text));
    IF TG_TABLE_NAME = 'forward_zones' THEN
        IF EXISTS (SELECT 1 FROM reverse_zones WHERE name = NEW.name) THEN
            RAISE EXCEPTION 'zone name already exists as a reverse zone';
        END IF;
    ELSIF EXISTS (SELECT 1 FROM forward_zones WHERE name = NEW.name) THEN
        RAISE EXCEPTION 'zone name already exists as a forward zone';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER forward_zones_validate_name_kind
BEFORE INSERT OR UPDATE OF name ON forward_zones
FOR EACH ROW EXECUTE FUNCTION validate_zone_name_kind_uniqueness();
CREATE TRIGGER reverse_zones_validate_name_kind
BEFORE INSERT OR UPDATE OF name ON reverse_zones
FOR EACH ROW EXECUTE FUNCTION validate_zone_name_kind_uniqueness();

-- Built-in presentation is derived from validated data at read/export time.
UPDATE records
SET rendered = NULL
WHERE type_id IN (SELECT id FROM record_types WHERE built_in);

ALTER TABLE host_attachments DROP CONSTRAINT host_attachment_unique_with_mac;
CREATE UNIQUE INDEX host_attachments_identity_unique
    ON host_attachments (host_id, network_id, mac_address) NULLS NOT DISTINCT;

ALTER TABLE communities DROP CONSTRAINT communities_policy_name_unique;
ALTER TABLE communities ADD CONSTRAINT communities_network_name_unique UNIQUE (network_id, name);

ALTER TABLE host_attachments DROP CONSTRAINT host_attachments_network_id_fkey;
ALTER TABLE host_attachments
    ADD CONSTRAINT host_attachments_network_id_fkey
    FOREIGN KEY (network_id) REFERENCES networks(id) ON DELETE RESTRICT;

ALTER TABLE hosts DROP CONSTRAINT hosts_zone_id_fkey;
ALTER TABLE hosts
    ADD CONSTRAINT hosts_zone_id_fkey
    FOREIGN KEY (zone_id) REFERENCES forward_zones(id) ON DELETE RESTRICT;

ALTER TABLE forward_zone_nameservers DROP CONSTRAINT forward_zone_nameservers_nameserver_id_fkey;
ALTER TABLE forward_zone_nameservers
    ADD CONSTRAINT forward_zone_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE RESTRICT;
ALTER TABLE reverse_zone_nameservers DROP CONSTRAINT reverse_zone_nameservers_nameserver_id_fkey;
ALTER TABLE reverse_zone_nameservers
    ADD CONSTRAINT reverse_zone_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE RESTRICT;
ALTER TABLE forward_zone_delegation_nameservers DROP CONSTRAINT forward_zone_delegation_nameservers_nameserver_id_fkey;
ALTER TABLE forward_zone_delegation_nameservers
    ADD CONSTRAINT forward_zone_delegation_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE RESTRICT;
ALTER TABLE reverse_zone_delegation_nameservers DROP CONSTRAINT reverse_zone_delegation_nameservers_nameserver_id_fkey;
ALTER TABLE reverse_zone_delegation_nameservers
    ADD CONSTRAINT reverse_zone_delegation_nameservers_nameserver_id_fkey
    FOREIGN KEY (nameserver_id) REFERENCES nameservers(id) ON DELETE RESTRICT;

ALTER TABLE ip_addresses ADD CONSTRAINT ip_addresses_host_address_unique UNIQUE (host_id, address);
ALTER TABLE ptr_overrides
    ADD CONSTRAINT ptr_overrides_assignment_fkey
    FOREIGN KEY (host_id, address) REFERENCES ip_addresses(host_id, address) ON DELETE CASCADE;

-- Keep generated DNS data distinct from records created by users. This makes
-- assignment cleanup exact even when a user creates an identical-looking RR.
CREATE TABLE managed_ip_records (
    ip_address_id UUID NOT NULL REFERENCES ip_addresses(id) ON DELETE CASCADE,
    record_id UUID NOT NULL UNIQUE REFERENCES records(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('forward', 'ptr')),
    PRIMARY KEY (ip_address_id, record_id)
);

CREATE FUNCTION validate_attachment_inventory() RETURNS trigger AS $$
DECLARE
    attachment_host UUID;
    attachment_network CIDR;
    network_frozen BOOLEAN;
BEGIN
    SELECT a.host_id, n.network, n.frozen
      INTO attachment_host, attachment_network, network_frozen
      FROM host_attachments a
      JOIN networks n ON n.id = a.network_id
     WHERE a.id = NEW.attachment_id
     FOR SHARE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'attachment does not exist';
    END IF;
    IF attachment_host <> NEW.host_id THEN
        RAISE EXCEPTION 'IP assignment host does not match attachment host';
    END IF;
    IF NOT attachment_network >>= NEW.address THEN
        RAISE EXCEPTION 'IP address is outside the attachment network';
    END IF;
    IF network_frozen THEN
        RAISE EXCEPTION 'network is frozen';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ip_addresses_validate_attachment
BEFORE INSERT OR UPDATE ON ip_addresses
FOR EACH ROW EXECUTE FUNCTION validate_attachment_inventory();

CREATE FUNCTION validate_excluded_network_range() RETURNS trigger AS $$
DECLARE
    parent CIDR;
    network_frozen BOOLEAN;
BEGIN
    SELECT network, frozen INTO parent, network_frozen FROM networks WHERE id = NEW.network_id FOR SHARE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'network does not exist';
    END IF;
    IF family(NEW.start_ip) <> family(NEW.end_ip) OR NEW.start_ip > NEW.end_ip THEN
        RAISE EXCEPTION 'excluded range bounds are invalid';
    END IF;
    IF NOT parent >>= NEW.start_ip OR NOT parent >>= NEW.end_ip THEN
        RAISE EXCEPTION 'excluded range is outside its network';
    END IF;
    IF network_frozen THEN
        RAISE EXCEPTION 'network is frozen';
    END IF;
    IF EXISTS (
        SELECT 1 FROM network_excluded_ranges existing
        WHERE existing.network_id = NEW.network_id
          AND existing.id <> NEW.id
          AND existing.start_ip <= NEW.end_ip
          AND NEW.start_ip <= existing.end_ip
    ) THEN
        RAISE EXCEPTION 'excluded ranges cannot overlap';
    END IF;
    IF EXISTS (
        SELECT 1 FROM ip_addresses ip
        JOIN host_attachments attachment ON attachment.id = ip.attachment_id
        WHERE attachment.network_id = NEW.network_id
          AND ip.address BETWEEN NEW.start_ip AND NEW.end_ip
    ) THEN
        RAISE EXCEPTION 'excluded range contains an allocated address';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER network_excluded_ranges_validate
BEFORE INSERT OR UPDATE ON network_excluded_ranges
FOR EACH ROW EXECUTE FUNCTION validate_excluded_network_range();

CREATE FUNCTION validate_prefix_reservation() RETURNS trigger AS $$
DECLARE
    parent CIDR;
    network_frozen BOOLEAN;
BEGIN
    SELECT n.network, n.frozen INTO parent, network_frozen
      FROM host_attachments a JOIN networks n ON n.id = a.network_id
     WHERE a.id = NEW.attachment_id FOR SHARE;
    IF NOT FOUND OR NOT parent >>= NEW.prefix THEN
        RAISE EXCEPTION 'prefix reservation is outside the attachment network';
    END IF;
    IF family(NEW.prefix) <> 6 THEN
        RAISE EXCEPTION 'prefix reservations must be IPv6';
    END IF;
    IF network_frozen THEN
        RAISE EXCEPTION 'network is frozen';
    END IF;
    IF EXISTS (
        SELECT 1 FROM attachment_prefix_reservations existing
        JOIN host_attachments existing_attachment ON existing_attachment.id = existing.attachment_id
        JOIN host_attachments new_attachment ON new_attachment.id = NEW.attachment_id
        WHERE existing.id <> NEW.id
          AND existing_attachment.network_id = new_attachment.network_id
          AND (existing.prefix >>= NEW.prefix OR NEW.prefix >>= existing.prefix)
    ) THEN
        RAISE EXCEPTION 'prefix reservations cannot overlap';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachment_prefix_reservations_validate
BEFORE INSERT OR UPDATE ON attachment_prefix_reservations
FOR EACH ROW EXECUTE FUNCTION validate_prefix_reservation();

CREATE FUNCTION validate_attachment_community_network() RETURNS trigger AS $$
DECLARE
    attachment_network UUID;
    community_network UUID;
BEGIN
    SELECT network_id INTO attachment_network
      FROM host_attachments WHERE id = NEW.attachment_id FOR SHARE;
    SELECT network_id INTO community_network
      FROM communities WHERE id = NEW.community_id FOR SHARE;
    IF attachment_network IS NULL OR community_network IS NULL
       OR attachment_network <> community_network THEN
        RAISE EXCEPTION 'community must belong to the attachment network';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attachment_community_assignments_validate_network
BEFORE INSERT OR UPDATE ON attachment_community_assignments
FOR EACH ROW EXECUTE FUNCTION validate_attachment_community_network();

CREATE FUNCTION validate_host_community_network() RETURNS trigger AS $$
DECLARE
    ip_host UUID;
    ip_network UUID;
    community_network UUID;
    network_frozen BOOLEAN;
BEGIN
    SELECT ip.host_id, a.network_id, n.frozen
      INTO ip_host, ip_network, network_frozen
      FROM ip_addresses ip
      JOIN host_attachments a ON a.id = ip.attachment_id
      JOIN networks n ON n.id = a.network_id
     WHERE ip.id = NEW.ip_address_id FOR SHARE OF ip, a, n;
    SELECT network_id INTO community_network
      FROM communities WHERE id = NEW.community_id FOR SHARE;
    IF ip_host IS NULL OR community_network IS NULL
       OR ip_host <> NEW.host_id OR ip_network <> community_network THEN
        RAISE EXCEPTION 'host community assignment is inconsistent with its IP and network';
    END IF;
    IF network_frozen THEN
        RAISE EXCEPTION 'network is frozen';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER host_community_assignments_validate_network
BEFORE INSERT OR UPDATE ON host_community_assignments
FOR EACH ROW EXECUTE FUNCTION validate_host_community_network();

-- A frozen network is an immutable inventory snapshot. Enforce this below the
-- application layer as well, including deletes and cascading mutations.
CREATE FUNCTION reject_frozen_network_mutation() RETURNS trigger AS $$
BEGIN
    IF OLD.frozen AND NOT (TG_OP = 'UPDATE' AND NEW.frozen = FALSE) THEN
        RAISE EXCEPTION 'network is frozen';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER networks_reject_frozen_mutation
BEFORE UPDATE OR DELETE ON networks
FOR EACH ROW EXECUTE FUNCTION reject_frozen_network_mutation();

CREATE FUNCTION reject_frozen_community_mutation() RETURNS trigger AS $$
DECLARE
    is_frozen BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        SELECT frozen INTO is_frozen FROM networks WHERE id = OLD.network_id FOR SHARE;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP <> 'DELETE' THEN
        SELECT frozen INTO is_frozen FROM networks WHERE id = NEW.network_id FOR SHARE;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER communities_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON communities
FOR EACH ROW EXECUTE FUNCTION reject_frozen_community_mutation();

CREATE FUNCTION reject_frozen_host_community_mutation() RETURNS trigger AS $$
DECLARE
    is_frozen BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        SELECT n.frozen INTO is_frozen
          FROM ip_addresses ip
          JOIN host_attachments a ON a.id = ip.attachment_id
          JOIN networks n ON n.id = a.network_id
         WHERE ip.id = OLD.ip_address_id FOR SHARE OF n;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP <> 'DELETE' THEN
        SELECT n.frozen INTO is_frozen
          FROM ip_addresses ip
          JOIN host_attachments a ON a.id = ip.attachment_id
          JOIN networks n ON n.id = a.network_id
         WHERE ip.id = NEW.ip_address_id FOR SHARE OF n;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER host_community_assignments_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON host_community_assignments
FOR EACH ROW EXECUTE FUNCTION reject_frozen_host_community_mutation();

CREATE FUNCTION reject_frozen_host_attachment_mutation() RETURNS trigger AS $$
DECLARE
    is_frozen BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        SELECT frozen INTO is_frozen FROM networks WHERE id = OLD.network_id FOR SHARE;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP <> 'DELETE' THEN
        SELECT frozen INTO is_frozen FROM networks WHERE id = NEW.network_id FOR SHARE;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER host_attachments_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON host_attachments
FOR EACH ROW EXECUTE FUNCTION reject_frozen_host_attachment_mutation();

CREATE FUNCTION reject_frozen_attachment_child_mutation() RETURNS trigger AS $$
DECLARE
    old_attachment UUID;
    new_attachment UUID;
    is_frozen BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        old_attachment := OLD.attachment_id;
        SELECT n.frozen INTO is_frozen
          FROM host_attachments a JOIN networks n ON n.id = a.network_id
         WHERE a.id = old_attachment FOR SHARE OF n;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP <> 'DELETE' THEN
        new_attachment := NEW.attachment_id;
        SELECT n.frozen INTO is_frozen
          FROM host_attachments a JOIN networks n ON n.id = a.network_id
         WHERE a.id = new_attachment FOR SHARE OF n;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER ip_addresses_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON ip_addresses
FOR EACH ROW EXECUTE FUNCTION reject_frozen_attachment_child_mutation();
CREATE TRIGGER attachment_dhcp_identifiers_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON attachment_dhcp_identifiers
FOR EACH ROW EXECUTE FUNCTION reject_frozen_attachment_child_mutation();
CREATE TRIGGER attachment_prefix_reservations_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON attachment_prefix_reservations
FOR EACH ROW EXECUTE FUNCTION reject_frozen_attachment_child_mutation();
CREATE TRIGGER attachment_community_assignments_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON attachment_community_assignments
FOR EACH ROW EXECUTE FUNCTION reject_frozen_attachment_child_mutation();

CREATE FUNCTION reject_frozen_excluded_range_mutation() RETURNS trigger AS $$
DECLARE
    is_frozen BOOLEAN;
BEGIN
    IF TG_OP <> 'INSERT' THEN
        SELECT frozen INTO is_frozen FROM networks WHERE id = OLD.network_id FOR SHARE;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP <> 'DELETE' THEN
        SELECT frozen INTO is_frozen FROM networks WHERE id = NEW.network_id FOR SHARE;
        IF is_frozen THEN
            RAISE EXCEPTION 'network is frozen';
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER network_excluded_ranges_reject_frozen_mutation
BEFORE INSERT OR UPDATE OR DELETE ON network_excluded_ranges
FOR EACH ROW EXECUTE FUNCTION reject_frozen_excluded_range_mutation();
