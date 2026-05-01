-- Database schema for Daily Tip Server

CREATE TABLE IF NOT EXISTS api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    key_hash TEXT NOT NULL UNIQUE,
    client_name TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS topic_classes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    tipcard_type TEXT NOT NULL DEFAULT 'srs_tip',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS topics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    class_id INTEGER,
    prompt_template TEXT,
    daily_card_count INTEGER,
    daily_time_zone TEXT,
    daily_update_time TEXT,
    UNIQUE(name, class_id),
    FOREIGN KEY(class_id) REFERENCES topic_classes(id)
);

CREATE TABLE IF NOT EXISTS tipcards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    topic_id INTEGER NOT NULL,
    tipcard_type TEXT NOT NULL DEFAULT 'srs_tip',
    title TEXT,
    full_content TEXT NOT NULL,
    compressed_content TEXT NOT NULL,
    image_data TEXT NOT NULL DEFAULT '[]',
    pinned INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(topic_id) REFERENCES topics(id)
);

CREATE TABLE IF NOT EXISTS review_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL UNIQUE,
    algorithm_used TEXT NOT NULL, -- 'fsrs' or 'sm2'
    state_data TEXT NOT NULL, -- JSON
    status TEXT NOT NULL DEFAULT 'active', -- 'active', 'acknowledged', 'memorized', or 'dismissed'
    next_review_at DATETIME NOT NULL,
    FOREIGN KEY(card_id) REFERENCES tipcards(id)
);

CREATE TABLE IF NOT EXISTS llm_token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    model TEXT NOT NULL,
    purpose TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS passkeys (
    passkey_id BLOB PRIMARY KEY,
    user_id TEXT NOT NULL,
    passkey TEXT NOT NULL, -- JSON serialized webauthn_rs::Passkey
    FOREIGN KEY(user_id) REFERENCES users(id)
);
