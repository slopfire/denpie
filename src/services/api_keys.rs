use sqlx::SqlitePool;

use crate::{
    auth::AuthUser,
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

    pub async fn verify(&self, api_key: &str) -> AppResult<AuthUser> {
        api_keys::verify(&self.pool, api_key).await.map(|verified| {
            let _ = verified.client_name;
            AuthUser {
                id: verified.user_id,
                username: verified.username,
                role: verified.role,
                display_name: None, // API key auth doesn't usually need these
                avatar_data: None,
                build_sha: option_env!("DENPIE_BUILD_SHA")
                    .unwrap_or("unknown")
                    .to_string(),
            }
        })
    }

    pub async fn create(&self, user_id: &str, client_name: Option<String>) -> AppResult<String> {
        api_keys::create(&self.pool, user_id, client_name).await
    }

    pub async fn list(&self, user_id: &str) -> AppResult<Vec<ApiKeyInfo>> {
        api_keys::list(&self.pool, user_id).await
    }

    pub async fn delete(&self, user_id: &str, id: i64) -> AppResult<()> {
        api_keys::delete(&self.pool, user_id, id).await
    }
}
