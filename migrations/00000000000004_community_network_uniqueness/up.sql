ALTER TABLE communities
    DROP CONSTRAINT IF EXISTS communities_policy_name_unique;

ALTER TABLE communities
    ADD CONSTRAINT communities_network_name_unique UNIQUE (network_id, name);
