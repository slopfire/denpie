use sqlx::{Row, SqlitePool};
use std::path::Path;

use crate::domain::{grounding::ImageSourceKind, topic_visual};

pub async fn apply_schema_file(pool: &SqlitePool, schema_path: &Path) -> Result<(), sqlx::Error> {
    let schema = tokio::fs::read_to_string(schema_path).await?;
    for query in schema.split(';') {
        if !query.trim().is_empty() {
            sqlx::query(query).execute(pool).await?;
        }
    }
    Ok(())
}

pub async fn apply_schema_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT,
            role TEXT NOT NULL DEFAULT 'user',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS llm_token_usage (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT,
            model TEXT NOT NULL,
            purpose TEXT NOT NULL,
            prompt_tokens INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_settings (
            user_id TEXT PRIMARY KEY,
            llm_model TEXT NOT NULL,
            llm_grounding_model TEXT NOT NULL DEFAULT '',
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
            max_active_cards INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS daily_refresh_runs (
            user_id TEXT NOT NULL,
            topic_id INTEGER NOT NULL,
            tipcard_type TEXT NOT NULL,
            window_start DATETIME NOT NULL,
            refreshed_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY(user_id, topic_id, tipcard_type),
            FOREIGN KEY(user_id) REFERENCES users(id),
            FOREIGN KEY(topic_id) REFERENCES topics(id)
        )",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS tipcard_images (
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
        )",
    )
    .execute(pool)
    .await?;

    ensure_column(pool, "users", "password_hash", "TEXT").await?;
    ensure_column(pool, "users", "role", "TEXT NOT NULL DEFAULT 'user'").await?;
    ensure_column(
        pool,
        "users",
        "created_at",
        "DATETIME DEFAULT CURRENT_TIMESTAMP",
    )
    .await?;
    ensure_column(pool, "users", "display_name", "TEXT").await?;
    ensure_column(pool, "users", "avatar_data", "TEXT").await?;
    ensure_column(pool, "api_keys", "user_id", "TEXT").await?;
    ensure_column(pool, "topics", "class_id", "INTEGER").await?;
    ensure_column(pool, "topics", "user_id", "TEXT").await?;
    ensure_column(
        pool,
        "topics",
        "tipcard_type",
        "TEXT NOT NULL DEFAULT 'repeatable_tip'",
    )
    .await?;
    ensure_column(pool, "topics", "prompt_template", "TEXT").await?;
    ensure_column(pool, "topics", "daily_card_count", "INTEGER").await?;
    ensure_column(pool, "topics", "daily_time_zone", "TEXT").await?;
    ensure_column(pool, "topics", "daily_update_time", "TEXT").await?;
    ensure_column(pool, "topics", "compression_level", "TEXT").await?;
    ensure_column(pool, "topics", "icon_id", "TEXT").await?;
    backfill_topic_icons(pool).await?;
    ensure_column(pool, "topics", "color_hue", "INTEGER").await?;
    backfill_topic_colors(pool).await?;
    ensure_column(pool, "topics", "grounding_strategy", "TEXT").await?;
    ensure_column(pool, "topics", "image_strategy", "TEXT").await?;
    rebuild_topics_without_global_name_unique(pool).await?;
    ensure_column(pool, "tipcards", "user_id", "TEXT").await?;
    ensure_column(
        pool,
        "tipcards",
        "tipcard_type",
        "TEXT NOT NULL DEFAULT 'repeatable_tip'",
    )
    .await?;
    ensure_column(pool, "tipcards", "title", "TEXT").await?;
    ensure_column(pool, "tipcards", "use_image", "INTEGER NOT NULL DEFAULT 0").await?;
    ensure_column(pool, "tipcards", "image_query", "TEXT NOT NULL DEFAULT ''").await?;
    ensure_column(pool, "tipcards", "image_data", "TEXT NOT NULL DEFAULT '[]'").await?;
    ensure_column(pool, "tipcards", "pinned", "INTEGER NOT NULL DEFAULT 0").await?;
    ensure_column(
        pool,
        "review_states",
        "status",
        "TEXT NOT NULL DEFAULT 'active'",
    )
    .await?;
    ensure_column(pool, "review_states", "daily_refreshed_at", "DATETIME").await?;
    ensure_column(
        pool,
        "review_states",
        "repeats",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    backfill_review_states_repeats(pool).await?;
    ensure_column(pool, "llm_token_usage", "user_id", "TEXT").await?;
    ensure_column(
        pool,
        "user_settings",
        "llm_grounding_model",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "llm_grounding_reasoning_effort",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "llm_vision_model",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "grounding_strategy",
        "TEXT NOT NULL DEFAULT 'factual'",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "image_strategy",
        "TEXT NOT NULL DEFAULT 'none'",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "search_api_key",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "search_base_url",
        "TEXT NOT NULL DEFAULT 'https://api.tavily.com'",
    )
    .await?;
    ensure_column(
        pool,
        "user_settings",
        "image_sources",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    backfill_image_sources(pool).await?;

    drop_appearance_columns_from_user_settings(pool).await?;

    // Migrate tipcard_type from classes to topics if topics.tipcard_type is default and class exists
    let _ = sqlx::query(
        "UPDATE topics
         SET tipcard_type = (SELECT tipcard_type FROM topic_classes WHERE id = topics.class_id)
         WHERE class_id IS NOT NULL AND (tipcard_type IS NULL OR tipcard_type = 'repeatable_tip')",
    )
    .execute(pool)
    .await;

    sqlx::query("UPDATE tipcards SET tipcard_type = 'repeatable_tip' WHERE tipcard_type IS NULL OR tipcard_type = 'srs_tip'")
        .execute(pool)
        .await?;

    sqlx::query("UPDATE review_states SET status = 'active' WHERE status IS NULL")
        .execute(pool)
        .await?;

    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_topics_user_name ON topics(user_id, name) WHERE user_id IS NOT NULL")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tipcards_user_id ON tipcards(user_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tipcard_images_card_id ON tipcard_images(card_id)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_tipcard_images_user_id ON tipcard_images(user_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_tipcard_images_card_position
         ON tipcard_images(card_id, position)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_llm_token_usage_user_id ON llm_token_usage(user_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_daily_refresh_runs_user_id ON daily_refresh_runs(user_id)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_documents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            source_type TEXT NOT NULL,
            title TEXT NOT NULL,
            url TEXT,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await?;
    stage_legacy_document_topics(pool).await?;
    rebuild_user_documents_without_legacy_topic_id(pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS document_topics (
            document_id INTEGER NOT NULL,
            topic_id INTEGER NOT NULL,
            PRIMARY KEY(document_id, topic_id),
            FOREIGN KEY(document_id) REFERENCES user_documents(id),
            FOREIGN KEY(topic_id) REFERENCES topics(id)
        )",
    )
    .execute(pool)
    .await?;
    backfill_document_topics(pool).await?;
    sqlx::query("DROP TABLE IF EXISTS legacy_document_topics")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS image_pool (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            storage_path TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            byte_size INTEGER NOT NULL,
            name TEXT NOT NULL,
            description TEXT,
            tags TEXT NOT NULL DEFAULT '[]',
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )",
    )
    .execute(pool)
    .await?;
    ensure_column(pool, "image_pool", "tags", "TEXT NOT NULL DEFAULT '[]'").await?;
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS document_chunks USING fts5(
            document_id UNINDEXED,
            user_id UNINDEXED,
            chunk
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_documents_user_id ON user_documents(user_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_document_topics_topic_id ON document_topics(topic_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_image_pool_user_id ON image_pool(user_id)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn rebuild_topics_without_global_name_unique(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if !has_unique_single_column_index(pool, "topics", "name").await? {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS topics_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT,
            name TEXT NOT NULL,
            class_id INTEGER,
            tipcard_type TEXT NOT NULL DEFAULT 'repeatable_tip',
            prompt_template TEXT,
            daily_card_count INTEGER,
            daily_time_zone TEXT,
            daily_update_time TEXT,
            compression_level TEXT,
            icon_id TEXT,
            color_hue INTEGER,
            grounding_strategy TEXT,
            image_strategy TEXT,
            FOREIGN KEY(user_id) REFERENCES users(id),
            UNIQUE(user_id, name)
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO topics_new (
            id, user_id, name, class_id, tipcard_type, prompt_template,
            daily_card_count, daily_time_zone, daily_update_time, compression_level,
            icon_id, color_hue, grounding_strategy, image_strategy
        )
        SELECT id, user_id, name, class_id, tipcard_type, prompt_template,
               daily_card_count, daily_time_zone, daily_update_time, compression_level,
               icon_id, color_hue, grounding_strategy, image_strategy
        FROM topics",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE topics").execute(&mut *tx).await?;
    sqlx::query("ALTER TABLE topics_new RENAME TO topics")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn has_unique_single_column_index(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> Result<bool, sqlx::Error> {
    let pragma = format!("PRAGMA index_list({table})");
    let indexes = sqlx::query(&pragma).fetch_all(pool).await?;
    for index in indexes {
        let unique = index.try_get::<i64, _>("unique").unwrap_or(0) == 1;
        if !unique {
            continue;
        }
        let name = index.try_get::<String, _>("name")?;
        let info = format!("PRAGMA index_info({name})");
        let columns = sqlx::query(&info).fetch_all(pool).await?;
        if columns.len() == 1
            && columns[0]
                .try_get::<String, _>("name")
                .map(|name| name == column)
                .unwrap_or(false)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn backfill_topic_icons(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query("SELECT id FROM topics WHERE icon_id IS NULL OR TRIM(icon_id) = ''")
        .fetch_all(pool)
        .await?;
    for row in rows {
        let id = row.try_get::<i64, _>("id")?;
        let icon_id = topic_visual::DEFAULT_TOPIC_ICON.to_string();
        sqlx::query("UPDATE topics SET icon_id = ? WHERE id = ?")
            .bind(icon_id)
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn backfill_topic_colors(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let rows = sqlx::query("SELECT id, name FROM topics WHERE color_hue IS NULL")
        .fetch_all(pool)
        .await?;
    for row in rows {
        let id = row.try_get::<i64, _>("id")?;
        let name = row.try_get::<String, _>("name")?;
        let color_hue = topic_visual::color_hue_from_name(&name);
        sqlx::query("UPDATE topics SET color_hue = ? WHERE id = ?")
            .bind(color_hue)
            .bind(id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn backfill_review_states_repeats(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE review_states
         SET repeats = COALESCE(CAST(json_extract(state_data, '$.repeats') AS INTEGER), 0)
         WHERE repeats = 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn backfill_image_sources(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let default_sources = crate::domain::grounding::DEFAULT_IMAGE_SOURCES_SETTING;
    sqlx::query(
        "UPDATE user_settings
         SET image_sources = ?
         WHERE image_sources IS NULL OR TRIM(image_sources) = '' OR image_sources = '[]'",
    )
    .bind(default_sources)
    .execute(pool)
    .await?;

    let columns = sqlx::query("PRAGMA table_info(user_settings)")
        .fetch_all(pool)
        .await?;
    let has_legacy_hosts = columns.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "image_fetch_allowed_hosts")
            .unwrap_or(false)
    });
    if !has_legacy_hosts {
        return Ok(());
    }

    let rows = sqlx::query(
        "SELECT user_id, image_strategy, image_fetch_allowed_hosts
         FROM user_settings",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let user_id: String = row.try_get("user_id")?;
        let strategy: String = row.try_get("image_strategy")?;
        let legacy_hosts: String = row.try_get("image_fetch_allowed_hosts")?;
        let legacy_hosts = legacy_hosts.trim();
        if legacy_hosts.is_empty() {
            continue;
        }

        let mut sources = crate::domain::grounding::default_image_sources();
        match strategy.as_str() {
            "programmatic" => {
                for source in &mut sources {
                    source.enabled = source.kind == ImageSourceKind::Api
                        && ((source.id == "danbooru" && legacy_hosts.contains("danbooru"))
                            || (source.id == "safebooru" && legacy_hosts.contains("safebooru")));
                }
            }
            "agentic" if legacy_hosts != "danbooru.donmau.us,safebooru.org,cdn.donmau.us" => {
                if let Some(source) = sources
                    .iter_mut()
                    .find(|source| source.kind == ImageSourceKind::WebSearch)
                {
                    source.enabled = true;
                    source.download_hosts = legacy_hosts.to_string();
                }
            }
            _ => continue,
        }
        let setting = serde_json::to_string(&sources)
            .expect("built-in image source migration is serializable");
        sqlx::query("UPDATE user_settings SET image_sources = ? WHERE user_id = ?")
            .bind(setting)
            .bind(user_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn backfill_document_topics(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if has_column(pool, "user_documents", "topic_id").await? {
        sqlx::query(
            "INSERT OR IGNORE INTO document_topics (document_id, topic_id)
             SELECT docs.id, docs.topic_id
             FROM user_documents docs
             JOIN topics top ON top.id = docs.topic_id AND top.user_id = docs.user_id
             WHERE docs.topic_id IS NOT NULL",
        )
        .execute(pool)
        .await?;
    } else if has_table(pool, "legacy_document_topics").await? {
        sqlx::query(
            "INSERT OR IGNORE INTO document_topics (document_id, topic_id)
             SELECT document_id, topic_id FROM legacy_document_topics",
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn stage_legacy_document_topics(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    if !has_column(pool, "user_documents", "topic_id").await? {
        return Ok(());
    }

    sqlx::query("DROP TABLE IF EXISTS legacy_document_topics")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE legacy_document_topics (
            document_id INTEGER NOT NULL,
            topic_id INTEGER NOT NULL,
            PRIMARY KEY(document_id, topic_id)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO legacy_document_topics (document_id, topic_id)
         SELECT docs.id, docs.topic_id
         FROM user_documents docs
         JOIN topics top ON top.id = docs.topic_id AND top.user_id = docs.user_id
         WHERE docs.topic_id IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn rebuild_user_documents_without_legacy_topic_id(
    pool: &SqlitePool,
) -> Result<(), sqlx::Error> {
    if !has_column(pool, "user_documents", "topic_id").await? {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    sqlx::query("DROP TABLE IF EXISTS user_documents_without_legacy_topic")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE user_documents_without_legacy_topic (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id TEXT NOT NULL,
            source_type TEXT NOT NULL,
            title TEXT NOT NULL,
            url TEXT,
            content TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY(user_id) REFERENCES users(id)
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO user_documents_without_legacy_topic
            (id, user_id, source_type, title, url, content, created_at)
         SELECT id, user_id, source_type, title, url, content, created_at
         FROM user_documents",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE user_documents")
        .execute(&mut *tx)
        .await?;
    sqlx::query("ALTER TABLE user_documents_without_legacy_topic RENAME TO user_documents")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn ensure_column(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), sqlx::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&pragma).fetch_all(pool).await?;
    let exists = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    });
    if !exists {
        let statement = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
        sqlx::query(&statement).execute(pool).await?;
    }
    Ok(())
}

async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> Result<bool, sqlx::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&pragma).fetch_all(pool).await?;
    Ok(rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == column)
            .unwrap_or(false)
    }))
}

async fn has_table(pool: &SqlitePool, table: &str) -> Result<bool, sqlx::Error> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(count == 1)
}

/// Remove appearance columns (color_scheme, transparency, blur_intensity) from
/// user_settings on existing databases. These are now device-local (browser
/// LocalStorage), not per-account. SQLite < 3.35 lacks DROP COLUMN, so rebuild
/// the table when any of the legacy columns are present.
async fn drop_appearance_columns_from_user_settings(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let pragma = sqlx::query("PRAGMA table_info(user_settings)")
        .fetch_all(pool)
        .await?;
    let has_color = pragma.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "color_scheme")
            .unwrap_or(false)
    });
    if !has_color {
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_settings_new (
            user_id TEXT PRIMARY KEY,
            llm_model TEXT NOT NULL,
            llm_grounding_model TEXT NOT NULL DEFAULT '',
            llm_vision_model TEXT NOT NULL DEFAULT '',
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
            max_active_cards INTEGER NOT NULL DEFAULT 0,
            grounding_strategy TEXT NOT NULL DEFAULT 'factual',
            image_strategy TEXT NOT NULL DEFAULT 'none',
            search_api_key TEXT NOT NULL DEFAULT '',
            search_base_url TEXT NOT NULL DEFAULT 'https://api.tavily.com',
            image_sources TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY(user_id) REFERENCES users(id)
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO user_settings_new (
            user_id, llm_model, llm_grounding_model, llm_vision_model, llm_compress_model,
            prompt_template, llm_api_key, llm_base_url, llm_compress_base_url,
            llm_reasoning_effort, llm_grounding_reasoning_effort,
            llm_compress_reasoning_effort, llm_compression_level, daily_time_zone,
            daily_update_time, max_active_cards, grounding_strategy, image_strategy,
            search_api_key, search_base_url, image_sources
        )
        SELECT
            user_id, llm_model, COALESCE(llm_grounding_model, ''),
            COALESCE(llm_vision_model, ''), llm_compress_model, prompt_template,
            llm_api_key, llm_base_url, llm_compress_base_url, llm_reasoning_effort,
            COALESCE(llm_grounding_reasoning_effort, ''), llm_compress_reasoning_effort,
            llm_compression_level, daily_time_zone, daily_update_time, max_active_cards,
            COALESCE(grounding_strategy, 'factual'), COALESCE(image_strategy, 'none'),
            COALESCE(search_api_key, ''), COALESCE(search_base_url, 'https://api.tavily.com'),
            COALESCE(image_sources, '[]')
        FROM user_settings",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE user_settings")
        .execute(&mut *tx)
        .await?;
    sqlx::query("ALTER TABLE user_settings_new RENAME TO user_settings")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
