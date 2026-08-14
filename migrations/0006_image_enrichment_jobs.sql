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

CREATE INDEX IF NOT EXISTS idx_card_image_jobs_ready
    ON card_image_jobs(status, available_at, created_at);

INSERT INTO card_image_jobs (card_id, user_id)
SELECT card.id, card.user_id
FROM tipcards card
WHERE card.use_image != 0
  AND BTRIM(card.image_query) != ''
  AND NOT EXISTS (
      SELECT 1 FROM tipcard_images image WHERE image.card_id = card.id
  )
ON CONFLICT (card_id) DO NOTHING;
