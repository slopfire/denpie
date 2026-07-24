use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::error::AppResult;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ImagePoolRecord {
    pub id: i64,
    pub user_id: String,
    pub storage_path: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags: String,
    pub created_at: DateTime<Utc>,
}
#[allow(clippy::too_many_arguments)]
pub async fn insert_pool_image(
    pool: &SqlitePool,
    user_id: &str,
    storage_path: &str,
    mime_type: &str,
    byte_size: i64,
    name: &str,
    description: Option<&str>,
    tags: &str,
) -> AppResult<i64> {
    Ok(sqlx::query(
        "INSERT INTO image_pool (user_id, storage_path, mime_type, byte_size, name, description, tags)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(storage_path)
    .bind(mime_type)
    .bind(byte_size)
    .bind(name)
    .bind(description)
    .bind(tags)
    .execute(pool)
    .await?
    .last_insert_rowid())
}

pub async fn list_pool_images(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<ImagePoolRecord>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, user_id, storage_path, mime_type, byte_size, name, description, tags, created_at
         FROM image_pool
         WHERE user_id = ?
         ORDER BY name ASC, id ASC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ImagePoolRecord {
            id: row.0,
            user_id: row.1,
            storage_path: row.2,
            mime_type: row.3,
            byte_size: row.4,
            name: row.5,
            description: row.6,
            tags: row.7,
            created_at: row.8,
        })
        .collect())
}

pub async fn find_pool_image(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
) -> AppResult<Option<ImagePoolRecord>> {
    let row = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            String,
            i64,
            String,
            Option<String>,
            String,
            DateTime<Utc>,
        ),
    >(
        "SELECT id, user_id, storage_path, mime_type, byte_size, name, description, tags, created_at
         FROM image_pool
         WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ImagePoolRecord {
        id: row.0,
        user_id: row.1,
        storage_path: row.2,
        mime_type: row.3,
        byte_size: row.4,
        name: row.5,
        description: row.6,
        tags: row.7,
        created_at: row.8,
    }))
}

pub async fn delete_pool_image(pool: &SqlitePool, user_id: &str, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM image_pool WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update the name, description, and tags of a pool image.
pub async fn update_pool_image_meta(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
    name: &str,
    description: Option<&str>,
    tags: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE image_pool SET name = ?, description = ?, tags = ? WHERE id = ? AND user_id = ?",
    )
    .bind(name)
    .bind(description)
    .bind(tags)
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a single tag from a pool image's tags JSON array. `tags_json` is the
/// current JSON array string; the caller passes the result of
/// `remove_tag_json` so this function just persists it.
pub async fn set_pool_image_tags(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
    tags: &str,
) -> AppResult<()> {
    sqlx::query("UPDATE image_pool SET tags = ? WHERE id = ? AND user_id = ?")
        .bind(tags)
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
