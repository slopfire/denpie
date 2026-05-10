use sqlx::SqlitePool;

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    pub password_hash: Option<String>,
    pub role: String,
    pub display_name: Option<String>,
    pub avatar_data: Option<String>,
}

pub async fn count(pool: &SqlitePool) -> AppResult<i64> {
    Ok(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?)
}

pub async fn list_ids(pool: &SqlitePool) -> AppResult<Vec<String>> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT id FROM users ORDER BY created_at ASC")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn has_unowned_rows(pool: &SqlitePool) -> AppResult<bool> {
    let api_keys = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM api_keys WHERE user_id IS NULL OR user_id = ''",
    )
    .fetch_one(pool)
    .await?;
    let topics = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM topics WHERE user_id IS NULL OR user_id = ''",
    )
    .fetch_one(pool)
    .await?;
    let tipcards = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tipcards WHERE user_id IS NULL OR user_id = ''",
    )
    .fetch_one(pool)
    .await?;
    let usage = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM llm_token_usage WHERE user_id IS NULL OR user_id = ''",
    )
    .fetch_one(pool)
    .await?;
    Ok(api_keys + topics + tipcards + usage > 0)
}

pub async fn setup_allowed(pool: &SqlitePool) -> AppResult<bool> {
    Ok(count(pool).await? == 0 || has_unowned_rows(pool).await?)
}

pub async fn create(
    pool: &SqlitePool,
    id: &str,
    username: &str,
    password_hash: &str,
    role: &str,
) -> AppResult<UserRecord> {
    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(id)
        .bind(username)
        .bind(password_hash)
        .bind(role)
        .execute(pool)
        .await?;

    Ok(UserRecord {
        id: id.to_string(),
        username: username.to_string(),
        password_hash: Some(password_hash.to_string()),
        role: role.to_string(),
        display_name: None,
        avatar_data: None,
    })
}

pub async fn find_by_username(pool: &SqlitePool, username: &str) -> AppResult<Option<UserRecord>> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, String, Option<String>, Option<String>)>(
        "SELECT id, username, password_hash, role, display_name, avatar_data FROM users WHERE username = ?",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| UserRecord {
        id: row.0,
        username: row.1,
        password_hash: row.2,
        role: row.3,
        display_name: row.4,
        avatar_data: row.5,
    }))
}

pub async fn find_by_id(pool: &SqlitePool, id: &str) -> AppResult<UserRecord> {
    let row = sqlx::query_as::<_, (String, String, Option<String>, String, Option<String>, Option<String>)>(
        "SELECT id, username, password_hash, role, display_name, avatar_data FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Auth("Invalid session".to_string()))?;

    Ok(UserRecord {
        id: row.0,
        username: row.1,
        password_hash: row.2,
        role: row.3,
        display_name: row.4,
        avatar_data: row.5,
    })
}

pub async fn first_admin(pool: &SqlitePool) -> AppResult<Option<UserRecord>> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
        ),
    >(
        "SELECT id, username, password_hash, role, display_name, avatar_data
         FROM users
         WHERE role = 'admin'
         ORDER BY created_at ASC
         LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| UserRecord {
        id: row.0,
        username: row.1,
        password_hash: row.2,
        role: row.3,
        display_name: row.4,
        avatar_data: row.5,
    }))
}

pub async fn claim_unowned_rows(pool: &SqlitePool, user_id: &str) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE api_keys SET user_id = ? WHERE user_id IS NULL OR user_id = ''")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE topics SET user_id = ? WHERE user_id IS NULL OR user_id = ''")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE tipcards
         SET user_id = COALESCE((SELECT user_id FROM topics WHERE topics.id = tipcards.topic_id), ?)
         WHERE user_id IS NULL OR user_id = ''",
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE llm_token_usage SET user_id = ? WHERE user_id IS NULL OR user_id = ''")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    display_name: Option<&str>,
    avatar_data: Option<&str>,
    password_hash: Option<&str>,
) -> AppResult<()> {
    let mut query = String::from("UPDATE users SET ");
    let mut params = Vec::new();

    if let Some(val) = display_name {
        query.push_str("display_name = ?, ");
        params.push(val);
    }
    if let Some(val) = avatar_data {
        query.push_str("avatar_data = ?, ");
        params.push(val);
    }
    if let Some(val) = password_hash {
        query.push_str("password_hash = ?, ");
        params.push(val);
    }

    if params.is_empty() {
        return Ok(());
    }

    // Remove trailing comma and space
    query.truncate(query.len() - 2);
    query.push_str(" WHERE id = ?");

    let mut q = sqlx::query(&query);
    for param in params {
        q = q.bind(param);
    }
    q.bind(id).execute(pool).await?;

    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    // Delete associated data
    sqlx::query("DELETE FROM api_keys WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM review_states WHERE card_id IN (SELECT id FROM tipcards WHERE user_id = ?)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM tipcards WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM topics WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM llm_token_usage WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_settings WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM passkeys WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}
