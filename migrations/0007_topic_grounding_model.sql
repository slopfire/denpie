ALTER TABLE topics
    ADD COLUMN IF NOT EXISTS grounding_model TEXT,
    ADD COLUMN IF NOT EXISTS grounding_reasoning_effort TEXT;
