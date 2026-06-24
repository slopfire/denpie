use std::collections::HashMap;

use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::error::{AppError, AppResult};

use super::{models::TipcardImageRecord, queries};

pub async fn list_images(
    pool: &SqlitePool,
    user_id: &str,
    card_id: i64,
) -> AppResult<Vec<TipcardImageRecord>> {
    let sql = format!(
        "{} WHERE user_id = ? AND card_id = ? ORDER BY position ASC, id ASC",
        queries::IMAGE_SELECT
    );
    let rows = sqlx::query_as::<_, TipcardImageRecord>(&sql)
        .bind(user_id)
        .bind(card_id)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn list_images_for_cards(
    pool: &SqlitePool,
    user_id: &str,
    card_ids: &[i64],
) -> AppResult<HashMap<i64, Vec<TipcardImageRecord>>> {
    if card_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut qb = QueryBuilder::<Sqlite>::new(
        "SELECT card_id, id, position, storage_path, mime_type, byte_size
         FROM tipcard_images
         WHERE user_id = ",
    );
    qb.push_bind(user_id);
    qb.push(" AND card_id IN (");
    let mut separated = qb.separated(", ");
    for id in card_ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(") ORDER BY card_id ASC, position ASC, id ASC");

    let rows = qb.build_query_as::<CardImageRow>().fetch_all(pool).await?;

    let mut map: HashMap<i64, Vec<TipcardImageRecord>> = HashMap::new();
    for row in rows {
        map.entry(row.card_id)
            .or_default()
            .push(TipcardImageRecord {
                id: row.id,
                position: row.position,
                storage_path: row.storage_path,
                mime_type: row.mime_type,
                byte_size: row.byte_size,
            });
    }
    Ok(map)
}

pub async fn find_image(
    pool: &SqlitePool,
    user_id: &str,
    id: i64,
) -> AppResult<TipcardImageRecord> {
    let sql = format!("{} WHERE user_id = ? AND id = ?", queries::IMAGE_SELECT);
    let row = sqlx::query_as::<_, TipcardImageRecord>(&sql)
        .bind(user_id)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Image not found".to_string()))?;
    Ok(row)
}

pub async fn replace_image_records(
    pool: &SqlitePool,
    user_id: &str,
    card_id: i64,
    images: &[TipcardImageRecord],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM tipcards WHERE id = ? AND user_id = ?")
            .bind(card_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Card not found".to_string()));
    }

    sqlx::query("DELETE FROM tipcard_images WHERE card_id = ? AND user_id = ?")
        .bind(card_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    for image in images {
        sqlx::query(
            "INSERT INTO tipcard_images (user_id, card_id, position, storage_path, mime_type, byte_size)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(card_id)
        .bind(image.position)
        .bind(&image.storage_path)
        .bind(&image.mime_type)
        .bind(image.byte_size)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE tipcards SET image_data = '[]' WHERE id = ? AND user_id = ?")
        .bind(card_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct CardImageRow {
    card_id: i64,
    id: i64,
    position: i64,
    storage_path: String,
    mime_type: String,
    byte_size: i64,
}
