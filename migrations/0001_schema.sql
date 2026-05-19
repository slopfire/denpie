-- Database schema for Denpie

CREATE TABLE IF NOT EXISTS api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    client_name TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(user_id) REFERENCES users(id)
);

CREATE TABLE IF NOT EXISTS topics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
    prompt_template TEXT,
    daily_card_count INTEGER,
    daily_time_zone TEXT,
    daily_update_time TEXT,
    compression_level TEXT,
    FOREIGN KEY(user_id) REFERENCES users(id),
    UNIQUE(user_id, name)
);

CREATE TABLE IF NOT EXISTS tipcards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    topic_id INTEGER NOT NULL,
    tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
    title TEXT,
    full_content TEXT NOT NULL,
    compressed_content TEXT NOT NULL,
    image_data TEXT NOT NULL DEFAULT '[]',
    pinned INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(user_id) REFERENCES users(id),
    FOREIGN KEY(topic_id) REFERENCES topics(id)
);

CREATE TABLE IF NOT EXISTS review_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL UNIQUE,
    algorithm_used TEXT NOT NULL, -- 'fsrs' or 'sm2'
    state_data TEXT NOT NULL, -- JSON
    status TEXT NOT NULL DEFAULT 'active', -- 'active', 'acknowledged', 'memorized', or 'dismissed'
    daily_refreshed_at DATETIME,
    next_review_at DATETIME NOT NULL,
    FOREIGN KEY(card_id) REFERENCES tipcards(id)
);

CREATE TABLE IF NOT EXISTS tipcard_images (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    card_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(user_id) REFERENCES users(id),
    FOREIGN KEY(card_id) REFERENCES tipcards(id)
);

CREATE TABLE IF NOT EXISTS llm_token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    model TEXT NOT NULL,
    purpose TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    role TEXT NOT NULL DEFAULT 'user',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_settings (
    user_id TEXT PRIMARY KEY,
    llm_model TEXT NOT NULL,
    llm_compress_model TEXT NOT NULL,
    prompt_template TEXT NOT NULL,
    llm_api_key TEXT NOT NULL,
    llm_base_url TEXT NOT NULL,
    llm_compress_base_url TEXT NOT NULL,
    llm_reasoning_effort TEXT NOT NULL,
    llm_compress_reasoning_effort TEXT NOT NULL,
    llm_compression_level TEXT NOT NULL,
    color_scheme TEXT NOT NULL,
    transparency TEXT NOT NULL,
    blur_intensity TEXT NOT NULL,
    daily_time_zone TEXT NOT NULL,
    daily_update_time TEXT NOT NULL,
    max_active_cards INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(user_id) REFERENCES users(id)
);

CREATE TABLE IF NOT EXISTS passkeys (
    passkey_id BLOB PRIMARY KEY,
    user_id TEXT NOT NULL,
    passkey TEXT NOT NULL, -- JSON serialized webauthn_rs::Passkey
    FOREIGN KEY(user_id) REFERENCES users(id)
);
