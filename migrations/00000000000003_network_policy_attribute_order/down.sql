DROP INDEX IF EXISTS network_policies_community_template_pattern_unique;

DELETE FROM network_policy_attributes
WHERE name = 'isolated'
  AND description = 'The network uses client isolation.';

ALTER TABLE network_policy_attribute_values
    DROP COLUMN IF EXISTS position;
