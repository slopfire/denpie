ALTER TABLE topics
    ADD COLUMN IF NOT EXISTS grounding_behavior TEXT;

ALTER TABLE user_settings
    ADD COLUMN IF NOT EXISTS grounding_behavior TEXT NOT NULL DEFAULT 'standard';

UPDATE user_settings
SET grounding_behavior = CASE
    WHEN grounding_strategy = 'agentic' THEN 'agentic'
    WHEN grounding_strategy = 'create_and_ground'
        OR grounding_verification_mode = 'thorough' THEN 'fact_check'
    ELSE 'standard'
END;

UPDATE topics
SET grounding_behavior = CASE
    WHEN grounding_strategy = 'agentic' THEN 'agentic'
    WHEN grounding_strategy = 'create_and_ground'
        OR grounding_verification_mode = 'thorough' THEN 'fact_check'
    WHEN grounding_strategy IN ('factual', 'rag')
        OR grounding_verification_mode = 'standard' THEN 'standard'
    ELSE NULL
END;
