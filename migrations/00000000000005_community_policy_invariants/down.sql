DROP TRIGGER networks_enforce_community_policy ON networks;
DROP FUNCTION enforce_network_community_policy();
DROP TRIGGER communities_enforce_policy ON communities;
DROP FUNCTION enforce_community_policy();
ALTER TABLE networks DROP CONSTRAINT networks_community_limit_nonnegative;
ALTER TABLE communities DROP CONSTRAINT communities_policy_id_fkey;
ALTER TABLE communities ADD CONSTRAINT communities_policy_id_fkey
    FOREIGN KEY (policy_id) REFERENCES network_policies(id) ON DELETE CASCADE;
