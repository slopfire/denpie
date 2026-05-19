use std::path::Path;

use axum::http::StatusCode;
use base64::{engine::general_purpose::STANDARD, Engine};
use rand::{distributions::Alphanumeric, Rng};
use sqlx::SqlitePool;
use tokio::fs;

use crate::{
    db::repositories::tipcards::{self, TipcardImageRecord},
};

type StatusResult<T> = Result<T, (StatusCode, String)>;

pub async fn replace_card_images(
    pool: &SqlitePool,
    image_dir: &Path,
    user_id: &str,
    card_id: i64,
    image_data: Vec<String>,
) -> StatusResult<()> {
    fs::create_dir_all(image_dir)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let old_images = tipcards::list_images(pool, user_id, card_id)
        .await
        .map_err(|err| err.into_status_body())?;
    let mut new_records = Vec::new();
    for (position, data_url) in image_data.iter().enumerate() {
        let parsed = parse_data_url(data_url)?;
        let name = random_image_name(card_id, position, parsed.extension);
        fs::write(image_dir.join(&name), &parsed.bytes)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        new_records.push(TipcardImageRecord {
            id: 0,
            position: position as i64,
            storage_path: name,
            mime_type: parsed.mime_type.to_string(),
            byte_size: parsed.bytes.len() as i64,
        });
    }
    tipcards::replace_image_records(pool, user_id, card_id, &new_records)
        .await
        .map_err(|err| err.into_status_body())?;
    for image in old_images {
        let _ = fs::remove_file(image_dir.join(image.storage_path)).await;
    }
    Ok(())
}

pub async fn migrate_legacy_images(pool: &SqlitePool, image_dir: &Path) -> StatusResult<()> {
    fs::create_dir_all(image_dir)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, user_id, image_data
         FROM tipcards
         WHERE image_data IS NOT NULL AND image_data != '' AND image_data != '[]'",
    )
    .fetch_all(pool)
    .await
    .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    for (card_id, user_id, raw) in rows {
        let existing = tipcards::list_images(pool, &user_id, card_id)
            .await
            .map_err(|err| err.into_status_body())?;
        if !existing.is_empty() {
            continue;
        }
        let Ok(images) = serde_json::from_str::<Vec<String>>(&raw) else {
            continue;
        };
        replace_card_images(pool, image_dir, &user_id, card_id, images).await?;
    }
    Ok(())
}

struct ParsedImage<'a> {
    mime_type: &'a str,
    extension: &'a str,
    bytes: Vec<u8>,
}

fn parse_data_url(value: &str) -> StatusResult<ParsedImage<'_>> {
    let Some((header, payload)) = value.split_once(',') else {
        return Err((StatusCode::BAD_REQUEST, "Invalid image data URL".to_string()));
    };
    let mime_type = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid image data URL".to_string()))?;
    let extension = match mime_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Only PNG, JPEG, WebP, or GIF data URLs are supported".to_string(),
            ))
        }
    };
    let bytes = STANDARD
        .decode(payload)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid base64 image data".to_string()))?;
    Ok(ParsedImage {
        mime_type,
        extension,
        bytes,
    })
}

fn random_image_name(card_id: i64, position: usize, extension: &str) -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(18)
        .map(char::from)
        .collect();
    format!("{card_id}-{position}-{suffix}.{extension}")
}
