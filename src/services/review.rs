use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use crate::{db::repositories::reviews, domain, error::AppResult, scheduling::SchedulingState};

#[derive(Clone)]
pub struct ReviewService {
    pool: SqlitePool,
}

impl ReviewService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn apply_review(
        &self,
        user_id: &str,
        card_id: i64,
        grade: u8,
        action: &str,
    ) -> AppResult<()> {
        let row = reviews::load_for_card(&self.pool, user_id, card_id).await?;

        if domain::tipcard::is_queue_tipcard(&row.tipcard_type)
            || row.tipcard_type == "repeatable_tip"
        {
            let action = action.trim();
            let (new_state_json, repeats, status, next_review) = match action {
                "acknowledge" | "acknowledged" => {
                    let mut repeat_state =
                        domain::review::RepeatableState::try_from_state_data(&row.state_data)?;
                    let next_review = domain::review::next_review(
                        &mut repeat_state.scheduling_state,
                        grade.max(3),
                    );
                    (
                        serde_json::to_string(&repeat_state)?,
                        repeat_state.repeats,
                        "active".to_string(),
                        next_review,
                    )
                }
                "memorize" => {
                    let mut repeat_state =
                        domain::review::RepeatableState::try_from_state_data(&row.state_data)?;
                    repeat_state.repeats += 1;
                    let next_review =
                        domain::review::next_review(&mut repeat_state.scheduling_state, 5);
                    (
                        serde_json::to_string(&repeat_state)?,
                        repeat_state.repeats,
                        "active".to_string(),
                        next_review,
                    )
                }
                "dismiss" => (
                    row.state_data,
                    row.repeats,
                    "dismissed".to_string(),
                    Utc::now() + Duration::days(36500),
                ),
                _ => {
                    let mut repeat_state =
                        domain::review::RepeatableState::try_from_state_data(&row.state_data)?;
                    repeat_state.repeats += 1;
                    let next_review = domain::review::next_review(
                        &mut repeat_state.scheduling_state,
                        if grade == 0 { 1 } else { grade.min(2) },
                    );
                    (
                        serde_json::to_string(&repeat_state)?,
                        repeat_state.repeats,
                        "active".to_string(),
                        next_review,
                    )
                }
            };

            reviews::update_queue_state(
                &self.pool,
                user_id,
                card_id,
                new_state_json,
                repeats,
                status,
                next_review,
            )
            .await?;
            return Ok(());
        }

        let mut scheduling_state: SchedulingState = serde_json::from_str(&row.state_data)?;
        let next_review = domain::review::next_review(&mut scheduling_state, grade);
        let new_state_json = serde_json::to_string(&scheduling_state)?;
        reviews::update_review_schedule(
            &self.pool,
            user_id,
            card_id,
            new_state_json,
            row.repeats,
            next_review,
        )
        .await
    }
}
