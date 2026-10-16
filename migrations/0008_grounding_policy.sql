ALTER TABLE topics
    ADD COLUMN IF NOT EXISTS grounding_source_mode TEXT,
    ADD COLUMN IF NOT EXISTS grounding_verification_mode TEXT,
    ADD COLUMN IF NOT EXISTS grounding_search_profile JSONB;

ALTER TABLE user_settings
    ADD COLUMN IF NOT EXISTS grounding_source_mode TEXT NOT NULL DEFAULT 'model',
    ADD COLUMN IF NOT EXISTS grounding_verification_mode TEXT NOT NULL DEFAULT 'standard',
    ADD COLUMN IF NOT EXISTS grounding_search_profile JSONB NOT NULL DEFAULT '{"depth":"balanced","freshness":"any","guidance":"","include_domains":[],"failure_policy":"stop"}'::jsonb;

UPDATE user_settings SET
    grounding_source_mode = CASE grounding_strategy
        WHEN 'agentic' THEN 'web'
        WHEN 'create_and_ground' THEN 'web'
        WHEN 'rag' THEN 'documents'
        ELSE 'model'
    END,
    grounding_verification_mode = CASE grounding_strategy
        WHEN 'create_and_ground' THEN 'thorough'
        ELSE 'standard'
    END;

UPDATE topics SET
    grounding_source_mode = CASE grounding_strategy
        WHEN 'agentic' THEN 'web'
        WHEN 'create_and_ground' THEN 'web'
        WHEN 'rag' THEN 'documents'
        WHEN 'factual' THEN 'model'
        ELSE NULL
    END,
    grounding_verification_mode = CASE grounding_strategy
        WHEN 'create_and_ground' THEN 'thorough'
        WHEN 'agentic' THEN 'standard'
        WHEN 'rag' THEN 'standard'
        WHEN 'factual' THEN 'standard'
        ELSE NULL
    END;

UPDATE user_settings
SET search_provider = 'native'
WHERE search_api_key = '' AND search_provider IN ('tavily', 'firecrawl');

ALTER TABLE tipcards
    ADD COLUMN IF NOT EXISTS grounding_status TEXT NOT NULL DEFAULT 'not_applicable',
    ADD COLUMN IF NOT EXISTS grounding_source_mode TEXT NOT NULL DEFAULT 'model';

CREATE TABLE IF NOT EXISTS tipcard_grounding_evidence (
    id BIGSERIAL PRIMARY KEY,
    card_id BIGINT NOT NULL REFERENCES tipcards(id) ON DELETE CASCADE,
    position BIGINT NOT NULL,
    source_type TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT,
    document_id BIGINT,
    excerpt TEXT NOT NULL DEFAULT '',
    retrieved_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(card_id, position)
);

CREATE INDEX IF NOT EXISTS idx_tipcard_grounding_evidence_card_id
    ON tipcard_grounding_evidence(card_id, position);
