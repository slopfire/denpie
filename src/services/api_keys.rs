use sqlx::SqlitePool;

use crate::{
    db::repositories::api_keys::{self, ApiKeyInfo},
    error::AppResult,
};

#[derive(Clone)]
pub struct ApiKeyService {
    pool: SqlitePool,
}

impl ApiKeyService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn verify(&self, api_key: &str) -> AppResult<String> {
        api_keys::verify(&self.pool, api_key).await
    }

    pub async fn create(&self, client_name: Option<String>) -> AppResult<String> {
        api_keys::create(&self.pool, client_name).await
    }

    pub async fn list(&self) -> AppResult<Vec<ApiKeyInfo>> {
        api_keys::list(&self.pool).await
    }

    pub async fn delete(&self, id: i64) -> AppResult<()> {
        api_keys::delete(&self.pool, id).await
    }
}
