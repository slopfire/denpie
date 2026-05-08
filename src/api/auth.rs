use crate::AppState;

use crate::auth::AuthUser;

use super::{pb, types::ApiResult};

pub(crate) async fn require_api_key(state: &AppState, api_key: &str) -> ApiResult<AuthUser> {
    state
        .api_keys
        .verify(api_key)
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
