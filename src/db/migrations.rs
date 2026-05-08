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
        "CREATE TABLE IF NOT EXISTS llm_token_usage (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
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

    ensure_column(pool, "topics", "class_id", "INTEGER").await?;
    ensure_column(pool, "topics", "prompt_template", "TEXT").await?;
    ensure_column(pool, "topics", "daily_card_count", "INTEGER").await?;
    ensure_column(pool, "topics", "daily_time_zone", "TEXT").await?;
    ensure_column(pool, "topics", "daily_update_time", "TEXT").await?;
    ensure_column(pool, "topics", "compression_level", "TEXT").await?;
    ensure_column(
        pool,
        "tipcards",
        "tipcard_type",
        "TEXT NOT NULL DEFAULT 'srs_tip'",
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

    sqlx::query(
        "INSERT OR IGNORE INTO topic_classes (name, tipcard_type) VALUES ('default', 'srs_tip')",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE topics
         SET class_id = (SELECT id FROM topic_classes WHERE name = 'default')
         WHERE class_id IS NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query("UPDATE tipcards SET tipcard_type = 'srs_tip' WHERE tipcard_type IS NULL")
        .execute(pool)
        .await?;

    sqlx::query("UPDATE review_states SET status = 'active' WHERE status IS NULL")
        .execute(pool)
        .await?;

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
