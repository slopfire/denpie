use crate::AppState;

use super::types::ApiResult;

pub async fn apply_review(
    state: &AppState,
    card_id: i64,
    grade: u8,
    action: &str,
) -> ApiResult<()> {
    state
        .reviews
        .apply_review(card_id, grade, action)
        .await
        .map_err(|err| err.into_status_body())
}
