-- Index the card-flow access patterns used by queue promotion, due selection,
-- cursor pagination, and batched image loading.

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

CREATE INDEX IF NOT EXISTS idx_tipcard_images_user_card_position
    ON tipcard_images(user_id, card_id, position, id);
