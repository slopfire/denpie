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

CREATE INDEX IF NOT EXISTS idx_api_idempotency_expiry
    ON api_idempotency_keys(expires_at)
    WHERE state = 'completed';
CREATE INDEX IF NOT EXISTS idx_api_idempotency_user_id
    ON api_idempotency_keys(user_id);
