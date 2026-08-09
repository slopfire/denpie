use chrono::{DateTime, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub user_id: String,
    pub client_name: String,
    pub created_at: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VerifiedApiKey {
    pub id: i64,
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub client_name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn verify(pool: &PgPool, api_key: &str) -> AppResult<VerifiedApiKey> {
    if api_key.trim().is_empty() {
        return Err(AppError::Auth("Missing API key".to_string()));
    }

    let row = sqlx::query_as::<
        _,
        (
            String,
            i64,
            String,
            String,
            String,
            Vec<String>,
            Option<DateTime<Utc>>,
        ),
    >(
        "WITH matched AS (
             SELECT id, user_id, client_name, scopes, expires_at
             FROM api_keys
             WHERE key_hash = $1
               AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
         ), touched AS (
             UPDATE api_keys
             SET last_used_at = CURRENT_TIMESTAMP
             WHERE id IN (SELECT id FROM matched)
               AND (last_used_at IS NULL OR last_used_at < CURRENT_TIMESTAMP - INTERVAL '1 minute')
         )
         SELECT matched.user_id, matched.id, u.username, u.role, matched.client_name, matched.scopes,
                matched.expires_at
         FROM matched
         JOIN users u ON u.id = matched.user_id",
    )
    .bind(hash_api_key(api_key))
    .fetch_optional(pool)
    .await?;

    row.map(|row| VerifiedApiKey {
        user_id: row.0,
        id: row.1,
        username: row.2,
        role: row.3,
        client_name: row.4,
        scopes: row.5,
        expires_at: row.6,
    })
    .ok_or_else(|| AppError::Auth("Invalid API key".to_string()))
}

pub async fn create(
    pool: &PgPool,
    user_id: &str,
    client_name: Option<String>,
) -> AppResult<String> {
    create_with_policy(pool, user_id, client_name, &["*".to_string()], None).await
}

pub async fn create_with_policy(
    pool: &PgPool,
    user_id: &str,
    client_name: Option<String>,
    scopes: &[String],
    expires_at: Option<DateTime<Utc>>,
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

    sqlx::query(
        "INSERT INTO api_keys (user_id, key_hash, client_name, scopes, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(hash_api_key(&api_key))
    .bind(client_name)
    .bind(scopes)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(api_key)
}

pub async fn list(pool: &PgPool, user_id: &str) -> AppResult<Vec<ApiKeyInfo>> {
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            String,
            String,
            String,
            Vec<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, user_id, client_name, COALESCE(CAST(created_at AS TEXT), ''), scopes,
                CAST(expires_at AS TEXT), CAST(last_used_at AS TEXT)
         FROM api_keys
         WHERE user_id = $1
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
            scopes: row.4,
            expires_at: row.5,
            last_used_at: row.6,
        })
        .collect())
}

pub async fn delete(pool: &PgPool, user_id: &str, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM api_keys WHERE user_id = $1 AND id = $2")
        .bind(user_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
