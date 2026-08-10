DROP INDEX IF EXISTS network_policies_community_template_pattern_unique;

ALTER TABLE network_policy_attribute_values
    DROP COLUMN IF EXISTS position;
