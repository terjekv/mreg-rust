-- Existing inventory needs an explicit policy before the new invariant can be
-- enabled. Abort rather than selecting a policy or deleting communities.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM communities c JOIN networks n ON n.id = c.network_id
        WHERE n.policy_id IS DISTINCT FROM c.policy_id
    ) THEN
        RAISE EXCEPTION 'Assign each network the policy used by its communities before upgrading (networks.policy_id); reconcile networks with multiple community policies';
    END IF;
    IF EXISTS (
        SELECT 1 FROM networks n WHERE n.max_communities IS NOT NULL
        AND (n.max_communities < 0 OR n.max_communities <
            (SELECT count(*) FROM communities c WHERE c.network_id = n.id))
    ) THEN
        RAISE EXCEPTION 'Raise networks.max_communities to cover existing communities before upgrading';
    END IF;
END;
$$;

ALTER TABLE networks ADD CONSTRAINT networks_community_limit_nonnegative
    CHECK (max_communities IS NULL OR max_communities >= 0);
ALTER TABLE communities DROP CONSTRAINT communities_policy_id_fkey;
ALTER TABLE communities ADD CONSTRAINT communities_policy_id_fkey
    FOREIGN KEY (policy_id) REFERENCES network_policies(id) ON DELETE RESTRICT;

CREATE FUNCTION enforce_community_policy() RETURNS trigger AS $$
DECLARE
    assigned_policy UUID;
    community_limit INTEGER;
    community_count BIGINT;
BEGIN
    -- All creators serialize on the same network, including direct SQL/imports.
    SELECT policy_id, max_communities INTO assigned_policy, community_limit
    FROM networks WHERE id = NEW.network_id FOR NO KEY UPDATE;
    IF assigned_policy IS DISTINCT FROM NEW.policy_id THEN
        RAISE EXCEPTION 'community policy must match the network policy' USING ERRCODE = '23514';
    END IF;
    SELECT count(*) INTO community_count FROM communities
    WHERE network_id = NEW.network_id AND id <> NEW.id;
    IF community_limit IS NOT NULL AND community_count >= community_limit THEN
        RAISE EXCEPTION 'network community limit has been reached' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER communities_enforce_policy
BEFORE INSERT OR UPDATE OF network_id, policy_id ON communities
FOR EACH ROW EXECUTE FUNCTION enforce_community_policy();

CREATE FUNCTION enforce_network_community_policy() RETURNS trigger AS $$
BEGIN
    -- Clearing a policy also clears its limit, including ON DELETE SET NULL.
    IF NEW.policy_id IS NULL THEN
        NEW.max_communities := NULL;
    END IF;
    IF EXISTS (SELECT 1 FROM communities WHERE network_id = NEW.id
               AND policy_id IS DISTINCT FROM NEW.policy_id) THEN
        RAISE EXCEPTION 'network policy is still referenced by communities' USING ERRCODE = '23514';
    END IF;
    IF NEW.max_communities IS NOT NULL AND NEW.max_communities <
       (SELECT count(*) FROM communities WHERE network_id = NEW.id) THEN
        RAISE EXCEPTION 'network community limit is below the existing community count' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER networks_enforce_community_policy
BEFORE UPDATE OF policy_id, max_communities ON networks
FOR EACH ROW EXECUTE FUNCTION enforce_network_community_policy();
