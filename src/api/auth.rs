use crate::AppState;
use axum::http::{HeaderMap, StatusCode, header};
use tower_sessions::Session;

use crate::services::api_keys::ApiPrincipal;

use super::{pb, types::ApiResult};

pub(crate) fn request_api_key(
    headers: &HeaderMap,
    body_api_key: &str,
) -> Result<String, (StatusCode, String)> {
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        let value = value.to_str().map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid Authorization header".to_string(),
            )
        })?;
        let (scheme, token) = value.split_once(' ').ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Authorization must use Bearer authentication".to_string(),
            )
        })?;
        if !scheme.eq_ignore_ascii_case("bearer") || token.trim().is_empty() {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Authorization must use a non-empty Bearer token".to_string(),
            ));
        }
        return Ok(token.trim().to_string());
    }
    Ok(body_api_key.to_string())
}

pub(crate) async fn require_api_key(state: &AppState, api_key: &str) -> ApiResult<ApiPrincipal> {
    state
        .api_keys
        .verify_principal(api_key)
        .await
        .map_err(|err| err.into_status_body())
}

/// Resolve API v1 auth for browser and key clients.
///
/// Priority: `Authorization: Bearer` → non-empty body `auth` → logged-in session.
/// Session principals hold full `*` scopes so the same-origin UI can call v1 after
/// normal login without storing a raw API key.
pub(crate) async fn resolve_principal(
    state: &AppState,
    headers: &HeaderMap,
    body_auth: &str,
    session: &Session,
) -> ApiResult<ApiPrincipal> {
    if headers.get(header::AUTHORIZATION).is_some() {
        let api_key = request_api_key(headers, "")?;
        return require_api_key(state, &api_key).await;
    }
    let body_auth = body_auth.trim();
    if !body_auth.is_empty() {
        return require_api_key(state, body_auth).await;
    }
    match crate::auth::current_user(state, session).await {
        Ok(user) => Ok(ApiPrincipal::from_session(user)),
        Err(_) => Err((
            StatusCode::UNAUTHORIZED,
            "Authentication required".to_string(),
        )),
    }
}

pub(crate) async fn create_scoped_api_key(
    state: &AppState,
    principal: &ApiPrincipal,
    req: pb::CreateApiKeyV1Request,
) -> ApiResult<String> {
    let expires_at = if req.expires_at.trim().is_empty() {
        None
    } else {
        let parsed = chrono::DateTime::parse_from_rfc3339(req.expires_at.trim())
            .map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "expires_at must be an RFC 3339 timestamp".to_string(),
                )
            })?
            .with_timezone(&chrono::Utc);
        if parsed <= chrono::Utc::now() {
            return Err((
                StatusCode::BAD_REQUEST,
                "expires_at must be in the future".to_string(),
            ));
        }
        Some(parsed)
    };
    let scopes = principal
        .validate_delegation(req.scopes, expires_at.as_ref())
        .map_err(|err| err.into_status_body())?;
    state
        .api_keys
        .create_scoped(
            &principal.user.id,
            Some(req.client_name),
            scopes,
            expires_at,
        )
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn create_raw_api_key(
    state: &AppState,
    user_id: &str,
    client_name: Option<String>,
) -> ApiResult<String> {
    state
        .api_keys
        .create(user_id, client_name)
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn list_api_keys_pb(state: &AppState, user_id: &str) -> ApiResult<pb::ApiKeys> {
    let rows = state
        .api_keys
        .list(user_id)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(pb::ApiKeys {
        keys: rows
            .into_iter()
            .map(|row| pb::ApiKeyInfo {
                id: row.id,
                client_name: row.client_name,
                created_at: row.created_at,
                scopes: row.scopes,
                expires_at: row.expires_at.unwrap_or_default(),
                last_used_at: row.last_used_at.unwrap_or_default(),
            })
            .collect(),
    })
}

pub(crate) async fn delete_api_key_by_id(
    state: &AppState,
    user_id: &str,
    id: i64,
) -> ApiResult<()> {
    state
        .api_keys
        .delete(user_id, id)
        .await
        .map_err(|err| err.into_status_body())
}
