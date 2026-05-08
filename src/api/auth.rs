use crate::AppState;

use super::{pb, types::ApiResult};

pub(crate) async fn require_api_key(state: &AppState, api_key: &str) -> ApiResult<()> {
    state
        .api_keys
        .verify(api_key)
        .await
        .map(|_| ())
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn create_raw_api_key(
    state: &AppState,
    client_name: Option<String>,
) -> ApiResult<String> {
    state
        .api_keys
        .create(client_name)
        .await
        .map_err(|err| err.into_status_body())
}

pub(crate) async fn list_api_keys_pb(state: &AppState) -> ApiResult<pb::ApiKeys> {
    let rows = state
        .api_keys
        .list()
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

pub(crate) async fn delete_api_key_by_id(state: &AppState, id: i64) -> ApiResult<()> {
    state
        .api_keys
        .delete(id)
        .await
        .map_err(|err| err.into_status_body())
}
