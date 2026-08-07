use chrono::{Duration, Utc};
use sqlx::PgPool;

use crate::{db::repositories::reviews, domain, error::AppResult, scheduling::SchedulingState};

#[derive(Clone)]
pub struct ReviewService {
    pool: PgPool,
}

impl ReviewService {
    pub fn new(pool: PgPool) -> Self {
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
            let (new_state_json, repeats, status, feedback, next_review) = match action {
                "acknowledge" | "acknowledged" => {
                    let mut repeat_state =
                        domain::review::RepeatableState::try_from_state_data(&row.state_data)?;
                    if row.tipcard_type == "repeatable_tip" {
                        repeat_state.repeats += 1;
                    }
                    let next_review = domain::review::next_review(
                        &mut repeat_state.scheduling_state,
                        grade.max(3),
                    );
                    (
                        serde_json::to_string(&repeat_state)?,
                        repeat_state.repeats,
                        "active".to_string(),
                        "learned",
                        next_review,
                    )
                }
                "learned" | "memorize" => {
                    let mut repeat_state =
                        domain::review::RepeatableState::try_from_state_data(&row.state_data)?;
                    repeat_state.repeats += 1;
                    let next_review =
                        domain::review::next_review(&mut repeat_state.scheduling_state, 5);
                    (
                        serde_json::to_string(&repeat_state)?,
                        repeat_state.repeats,
                        "active".to_string(),
                        "learned",
                        next_review,
                    )
                }
                "skip_known" => (
                    row.state_data,
                    row.repeats,
                    "dismissed".to_string(),
                    "known",
                    Utc::now() + Duration::days(36500),
                ),
                "skip_too_difficult" => (
                    row.state_data,
                    row.repeats,
                    "dismissed".to_string(),
                    "too_difficult",
                    Utc::now() + Duration::days(36500),
                ),
                "skip_not_interested" | "dismiss" => (
                    row.state_data,
                    row.repeats,
                    "dismissed".to_string(),
                    "not_interested",
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
                        "again",
                        next_review,
                    )
                }
            };
            let feedback = if row.tipcard_type == "repeatable_tip" {
                feedback
            } else {
                ""
            };

            reviews::update_queue_state(
                &self.pool,
                user_id,
                card_id,
                reviews::QueueReviewUpdate {
                    state_data: new_state_json,
                    repeats,
                    status,
                    feedback,
                    next_review_at: next_review,
                },
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
