-- PostgreSQL schema for Denpie. Runtime migrations are embedded from migrations/.

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    role TEXT NOT NULL DEFAULT 'user',
    display_name TEXT,
    avatar_data TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL UNIQUE,
    client_name TEXT NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT ARRAY['*']::TEXT[],
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS api_idempotency_keys (
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'in_progress'
        CHECK (state IN ('in_progress', 'completed')),
    status_code INTEGER,
    response_body BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL DEFAULT (CURRENT_TIMESTAMP + INTERVAL '24 hours'),
    PRIMARY KEY (actor_id, idempotency_key),
    CHECK (
        (state = 'in_progress' AND status_code IS NULL AND completed_at IS NULL)
        OR (state = 'completed' AND status_code IS NOT NULL AND completed_at IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS topics (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
    prompt_template TEXT,
    daily_card_count BIGINT,
    daily_time_zone TEXT,
    daily_update_time TEXT,
    compression_level TEXT,
    icon_id TEXT,
    color_hue BIGINT,
    grounding_strategy TEXT,
    grounding_model TEXT,
    grounding_reasoning_effort TEXT,
    image_strategy TEXT,
    UNIQUE(user_id, name)
);

CREATE TABLE IF NOT EXISTS tipcards (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    topic_id BIGINT NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
    title TEXT,
    full_content TEXT NOT NULL,
    compressed_content TEXT NOT NULL,
    use_image BIGINT NOT NULL DEFAULT 0,
    image_query TEXT NOT NULL DEFAULT '',
    image_data TEXT NOT NULL DEFAULT '[]',
    pinned BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS review_states (
    id BIGSERIAL PRIMARY KEY,
    card_id BIGINT NOT NULL UNIQUE REFERENCES tipcards(id) ON DELETE CASCADE,
    algorithm_used TEXT NOT NULL, -- 'sm2' ('fsrs' is a legacy alias only)
    state_data TEXT NOT NULL, -- JSON
    repeats BIGINT NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    feedback TEXT NOT NULL DEFAULT '',
    reviewed_at TIMESTAMPTZ,
    daily_refreshed_at TIMESTAMPTZ,
    next_review_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS tipcard_images (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    card_id BIGINT NOT NULL REFERENCES tipcards(id) ON DELETE CASCADE,
    position BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(card_id, position)
);

CREATE TABLE IF NOT EXISTS card_image_jobs (
    card_id BIGINT PRIMARY KEY REFERENCES tipcards(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts BIGINT NOT NULL DEFAULT 0,
    available_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    lease_until TIMESTAMPTZ,
    last_error TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (status IN ('pending', 'processing', 'completed', 'failed'))
);

CREATE TABLE IF NOT EXISTS llm_token_usage (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    purpose TEXT NOT NULL,
    prompt_tokens BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0,
    total_tokens BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_settings (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    llm_model TEXT NOT NULL,
    llm_grounding_model TEXT NOT NULL DEFAULT '',
    llm_vision_model TEXT NOT NULL,
    llm_compress_model TEXT NOT NULL,
    prompt_template TEXT NOT NULL,
    llm_api_key TEXT NOT NULL,
    llm_base_url TEXT NOT NULL,
    llm_compress_base_url TEXT NOT NULL,
    llm_reasoning_effort TEXT NOT NULL,
    llm_grounding_reasoning_effort TEXT NOT NULL DEFAULT '',
    llm_compress_reasoning_effort TEXT NOT NULL,
    llm_compression_level TEXT NOT NULL,
    daily_time_zone TEXT NOT NULL,
    daily_update_time TEXT NOT NULL,
    max_active_cards BIGINT NOT NULL DEFAULT 0,
    grounding_strategy TEXT NOT NULL DEFAULT 'factual',
    image_strategy TEXT NOT NULL DEFAULT 'none',
    search_provider TEXT NOT NULL DEFAULT 'tavily',
    scrape_provider TEXT NOT NULL DEFAULT 'scrapling',
    search_api_key TEXT NOT NULL DEFAULT '',
    search_base_url TEXT NOT NULL DEFAULT 'https://api.tavily.com',
    image_sources TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS daily_refresh_runs (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    topic_id BIGINT NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    tipcard_type TEXT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    refreshed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(user_id, topic_id, tipcard_type)
);

CREATE TABLE IF NOT EXISTS repeatable_daily_allowances (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    topic_id BIGINT NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    tipcard_type TEXT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    extra_cards BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, topic_id, tipcard_type, window_start)
);

CREATE TABLE IF NOT EXISTS passkeys (
    passkey_id BYTEA PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    passkey TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS user_documents (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS document_topics (
    document_id BIGINT NOT NULL REFERENCES user_documents(id) ON DELETE CASCADE,
    topic_id BIGINT NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    PRIMARY KEY(document_id, topic_id)
);

CREATE TABLE IF NOT EXISTS image_pool (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    storage_path TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    name TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS document_chunks (
    document_id BIGINT NOT NULL REFERENCES user_documents(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chunk TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_expires_at ON api_keys(expires_at)
    WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_api_idempotency_expiry
    ON api_idempotency_keys(expires_at)
    WHERE state = 'completed';
CREATE INDEX IF NOT EXISTS idx_api_idempotency_user_id
    ON api_idempotency_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_tipcards_user_id ON tipcards(user_id);
CREATE INDEX IF NOT EXISTS idx_tipcards_flow_cursor
    ON tipcards(user_id, pinned DESC, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_tipcards_topic_queue
    ON tipcards(user_id, topic_id, tipcard_type, created_at, id);
CREATE INDEX IF NOT EXISTS idx_review_states_active_due
    ON review_states(next_review_at, card_id)
    WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_review_states_pending_card
    ON review_states(card_id)
    WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_tipcard_images_card_id ON tipcard_images(card_id);
CREATE INDEX IF NOT EXISTS idx_tipcard_images_user_id ON tipcard_images(user_id);
CREATE INDEX IF NOT EXISTS idx_tipcard_images_user_card_position
    ON tipcard_images(user_id, card_id, position, id);
CREATE INDEX IF NOT EXISTS idx_card_image_jobs_ready
    ON card_image_jobs(status, available_at, created_at);
CREATE INDEX IF NOT EXISTS idx_llm_token_usage_user_id ON llm_token_usage(user_id);
CREATE INDEX IF NOT EXISTS idx_daily_refresh_runs_user_id ON daily_refresh_runs(user_id);
CREATE INDEX IF NOT EXISTS idx_user_documents_user_id ON user_documents(user_id);
CREATE INDEX IF NOT EXISTS idx_document_topics_topic_id ON document_topics(topic_id);
CREATE INDEX IF NOT EXISTS idx_image_pool_user_id ON image_pool(user_id);
CREATE INDEX IF NOT EXISTS idx_document_chunks_user_id ON document_chunks(user_id);
CREATE INDEX IF NOT EXISTS idx_document_chunks_fts
ON document_chunks USING GIN (to_tsvector('simple', chunk));
