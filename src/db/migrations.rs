use sqlx::{Row, SqlitePool};
use std::path::Path;

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
    ensure_column(pool, "llm_token_usage", "user_id", "TEXT").await?;

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
            FOREIGN KEY(user_id) REFERENCES users(id),
            UNIQUE(user_id, name)
        )",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO topics_new (
            id, user_id, name, class_id, tipcard_type, prompt_template,
            daily_card_count, daily_time_zone, daily_update_time, compression_level
        )
        SELECT id, user_id, name, class_id, tipcard_type, prompt_template,
               daily_card_count, daily_time_zone, daily_update_time, compression_level
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
