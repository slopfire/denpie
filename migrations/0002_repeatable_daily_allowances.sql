CREATE TABLE IF NOT EXISTS repeatable_daily_allowances (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    topic_id BIGINT NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    tipcard_type TEXT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    extra_cards BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (user_id, topic_id, tipcard_type, window_start)
);
