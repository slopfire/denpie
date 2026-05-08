use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn verify(pool: &SqlitePool, api_key: &str) -> AppResult<String> {
    if api_key.trim().is_empty() {
        return Err(AppError::Auth("Missing API key".to_string()));
    }

    let client_name: Option<String> =
        sqlx::query_scalar("SELECT client_name FROM api_keys WHERE key_hash = ?")
            .bind(hash_api_key(api_key))
            .fetch_optional(pool)
            .await?;

    client_name.ok_or_else(|| AppError::Auth("Invalid API key".to_string()))
}

pub async fn create(pool: &SqlitePool, client_name: Option<String>) -> AppResult<String> {
    let raw_key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let api_key = format!("sk_live_{raw_key}");
    let client_name = client_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default_client".to_string());

    sqlx::query("INSERT INTO api_keys (key_hash, client_name) VALUES (?, ?)")
        .bind(hash_api_key(&api_key))
        .bind(client_name)
        .execute(pool)
        .await?;

    Ok(api_key)
}

pub async fn list(pool: &SqlitePool) -> AppResult<Vec<ApiKeyInfo>> {
    let rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, client_name, COALESCE(CAST(created_at AS TEXT), '') FROM api_keys ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ApiKeyInfo {
            id: row.0,
            client_name: row.1,
            created_at: row.2,
        })
        .collect())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM api_keys WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
