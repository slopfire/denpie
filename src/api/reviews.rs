use crate::AppState;

use super::types::ApiResult;

pub async fn apply_review(
    state: &AppState,
    user_id: &str,
    card_id: i64,
    grade: u8,
    action: &str,
) -> ApiResult<()> {
    state
        .reviews
        .apply_review(user_id, card_id, grade, action)
        .await
        .map_err(|err| err.into_status_body())
}
