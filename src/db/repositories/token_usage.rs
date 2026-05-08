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
    model: &str,
    purpose: &str,
    usage: &TokenUsage,
) -> AppResult<()> {
    if usage.total_tokens <= 0 && usage.prompt_tokens <= 0 && usage.completion_tokens <= 0 {
        return Ok(());
    }

    sqlx::query(
        "INSERT INTO llm_token_usage (model, purpose, prompt_tokens, completion_tokens, total_tokens)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(model)
    .bind(purpose)
    .bind(usage.prompt_tokens)
    .bind(usage.completion_tokens)
    .bind(usage.total_tokens)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn aggregate_spend(pool: &SqlitePool) -> AppResult<TokenSpendRecord> {
    let daily = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(total_tokens), 0)
         FROM llm_token_usage
         WHERE date(created_at) = date('now')",
    )
    .fetch_one(pool)
    .await?;
    let monthly = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(total_tokens), 0)
         FROM llm_token_usage
         WHERE strftime('%Y-%m', created_at) = strftime('%Y-%m', 'now')",
    )
    .fetch_one(pool)
    .await?;
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(total_tokens), 0)
         FROM llm_token_usage",
    )
    .fetch_one(pool)
    .await?;

    Ok(TokenSpendRecord {
        daily,
        monthly,
        total,
    })
}
