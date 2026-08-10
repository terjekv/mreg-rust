ALTER TABLE communities
    DROP CONSTRAINT IF EXISTS communities_network_name_unique;

ALTER TABLE communities
    ADD CONSTRAINT communities_policy_name_unique UNIQUE (policy_id, name);
