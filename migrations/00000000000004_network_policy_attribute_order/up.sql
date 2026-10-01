ALTER TABLE network_policy_attribute_values
    ADD COLUMN position INTEGER NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX network_policies_community_template_pattern_unique
    ON network_policies (community_template_pattern)
    WHERE community_template_pattern IS NOT NULL;
