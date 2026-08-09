use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::{
    auth::AuthUser,
    db::repositories::api_keys::{self, ApiKeyInfo},
    error::AppResult,
};

pub const API_KEY_SCOPES: &[&str] = &[
    "cards:read",
    "cards:write",
    "reviews:write",
    "topics:read",
    "topics:write",
    "settings:read",
    "settings:write",
    "secrets:read",
    "keys:manage",
    "documents:read",
    "documents:write",
    "images:read",
    "images:write",
    "diagnostics:run",
];

#[derive(Clone, Debug)]
pub struct ApiPrincipal {
    pub user: AuthUser,
    key_id: i64,
    scopes: BTreeSet<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl ApiPrincipal {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains("*") || self.scopes.contains(scope)
    }

    pub fn scopes(&self) -> Vec<String> {
        self.scopes.iter().cloned().collect()
    }

    pub fn idempotency_actor_id(&self) -> String {
        format!("api_key:{}", self.key_id)
    }

    pub fn can_create_unrestricted_key(&self) -> bool {
        self.scopes.contains("*") && self.expires_at.is_none()
    }

    pub fn validate_delegation(
        &self,
        scopes: Vec<String>,
        expires_at: Option<&DateTime<Utc>>,
    ) -> AppResult<Vec<String>> {
        let scopes = normalize_scopes(scopes)?;
        for scope in &scopes {
            if !self.has_scope(scope) {
                return Err(crate::error::AppError::Forbidden(format!(
                    "API key cannot grant scope '{scope}' that it does not hold"
                )));
            }
        }
        if let Some(parent_expiry) = &self.expires_at {
            let Some(child_expiry) = expires_at else {
                return Err(crate::error::AppError::Forbidden(
                    "API key cannot create a credential that outlives it".to_string(),
                ));
            };
            if child_expiry > parent_expiry {
                return Err(crate::error::AppError::Forbidden(
                    "API key cannot create a credential that outlives it".to_string(),
                ));
            }
        }
        Ok(scopes)
    }
}

#[derive(Clone)]
pub struct ApiKeyService {
    pool: PgPool,
}

impl ApiKeyService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn verify(&self, api_key: &str) -> AppResult<AuthUser> {
        self.verify_principal(api_key)
            .await
            .map(|principal| principal.user)
    }

    pub async fn verify_principal(&self, api_key: &str) -> AppResult<ApiPrincipal> {
        api_keys::verify(&self.pool, api_key).await.map(|verified| {
            let _ = verified.client_name;
            ApiPrincipal {
                key_id: verified.id,
                scopes: verified.scopes.into_iter().collect(),
                expires_at: verified.expires_at,
                user: AuthUser {
                    id: verified.user_id,
                    username: verified.username,
                    role: verified.role,
                    display_name: None, // API key auth doesn't usually need these
                    avatar_data: None,
                    build_sha: option_env!("DENPIE_BUILD_SHA")
                        .unwrap_or("unknown")
                        .to_string(),
                },
            }
        })
    }

    pub async fn create(&self, user_id: &str, client_name: Option<String>) -> AppResult<String> {
        api_keys::create(&self.pool, user_id, client_name).await
    }

    pub async fn create_scoped(
        &self,
        user_id: &str,
        client_name: Option<String>,
        scopes: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> AppResult<String> {
        let scopes = normalize_scopes(scopes)?;
        api_keys::create_with_policy(&self.pool, user_id, client_name, &scopes, expires_at).await
    }

    pub async fn list(&self, user_id: &str) -> AppResult<Vec<ApiKeyInfo>> {
        api_keys::list(&self.pool, user_id).await
    }

    pub async fn delete(&self, user_id: &str, id: i64) -> AppResult<()> {
        api_keys::delete(&self.pool, user_id, id).await
    }
}

fn normalize_scopes(scopes: Vec<String>) -> AppResult<Vec<String>> {
    if scopes.is_empty() {
        return Err(crate::error::AppError::Validation(
            "At least one API key scope is required".to_string(),
        ));
    }
    let mut normalized = BTreeSet::new();
    for scope in scopes {
        let scope = scope.trim().to_ascii_lowercase();
        if scope == "*" {
            return Ok(vec!["*".to_string()]);
        }
        if !API_KEY_SCOPES.contains(&scope.as_str()) {
            return Err(crate::error::AppError::Validation(format!(
                "Unsupported API key scope: {scope}"
            )));
        }
        normalized.insert(scope);
    }
    Ok(normalized.into_iter().collect())
}
