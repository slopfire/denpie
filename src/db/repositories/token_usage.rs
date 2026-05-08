use sqlx::SqlitePool;

use crate::{error::AppResult, llm::TokenUsage};

#[derive(Clone, Debug)]
pub struct TokenSpendRecord {
    pub daily: i64,
    pub monthly: i64,
    pub total: i64,
}

pub async fn insert(
    pool: &SqlitePool,
    user_id: &str,
    model: &str,
    purpose: &str,
    usage: &TokenUsage,
) -> AppResult<()> {
    if usage.total_tokens <= 0 && usage.prompt_tokens <= 0 && usage.completion_tokens <= 0 {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO llm_token_usage (user_id, model, purpose, prompt_tokens, completion_tokens, total_tokens)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(model)
    .bind(purpose)
    .bind(usage.prompt_tokens)
    .bind(usage.completion_tokens)
    .bind(usage.total_tokens)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn aggregate_spend(pool: &SqlitePool, user_id: &str) -> AppResult<TokenSpendRecord> {
    let daily = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(total_tokens), 0)
         FROM llm_token_usage
         WHERE user_id = ? AND date(created_at) = date('now')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let monthly = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(total_tokens), 0)
         FROM llm_token_usage
         WHERE user_id = ? AND strftime('%Y-%m', created_at) = strftime('%Y-%m', 'now')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(total_tokens), 0)
         FROM llm_token_usage
         WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok(TokenSpendRecord {
        daily,
        monthly,
        total,
    })
}
