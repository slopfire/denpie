use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub user_id: String,
    pub client_name: String,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct VerifiedApiKey {
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub client_name: String,
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn verify(pool: &SqlitePool, api_key: &str) -> AppResult<VerifiedApiKey> {
    if api_key.trim().is_empty() {
        return Err(AppError::Auth("Missing API key".to_string()));
    }

    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT k.user_id, u.username, u.role, k.client_name
         FROM api_keys k
         JOIN users u ON u.id = k.user_id
         WHERE k.key_hash = ?",
    )
    .bind(hash_api_key(api_key))
    .fetch_optional(pool)
    .await?;

    row.map(|row| VerifiedApiKey {
        user_id: row.0,
        username: row.1,
        role: row.2,
        client_name: row.3,
    })
    .ok_or_else(|| AppError::Auth("Invalid API key".to_string()))
}

pub async fn create(
    pool: &SqlitePool,
    user_id: &str,
    client_name: Option<String>,
) -> AppResult<String> {
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

    sqlx::query("INSERT INTO api_keys (user_id, key_hash, client_name) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(hash_api_key(&api_key))
        .bind(client_name)
        .execute(pool)
        .await?;

    Ok(api_key)
}

pub async fn list(pool: &SqlitePool, user_id: &str) -> AppResult<Vec<ApiKeyInfo>> {
    let rows = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, user_id, client_name, COALESCE(CAST(created_at AS TEXT), '')
         FROM api_keys
         WHERE user_id = ?
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ApiKeyInfo {
            id: row.0,
            user_id: row.1,
            client_name: row.2,
            created_at: row.3,
        })
        .collect())
}

pub async fn delete(pool: &SqlitePool, user_id: &str, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM api_keys WHERE user_id = ? AND id = ?")
        .bind(user_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
