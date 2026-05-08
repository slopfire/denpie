use chrono::{Duration, Utc};
use sqlx::SqlitePool;

use crate::{
    db::repositories::reviews,
    domain,
    error::{AppError, AppResult},
    srs::SrsState,
};

#[derive(Clone)]
pub struct ReviewService {
    pool: SqlitePool,
}

impl ReviewService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn apply_review(&self, card_id: i64, grade: u8, action: &str) -> AppResult<()> {
        let row = reviews::load_for_card(&self.pool, card_id).await?;

        if domain::tipcard::is_queue_tipcard(&row.tipcard_type) {
            let action = action.trim();
            let (new_state_json, status, next_review) = match action {
                "acknowledge" | "acknowledged" => {
                    let mut repeat_state: domain::review::RepeatableState =
                        serde_json::from_str(&row.state_data).map_err(|_| {
                            AppError::Json(serde_json::Error::io(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid repeatable state data",
                            )))
                        })?;
                    let next_review =
                        domain::review::next_sm2_review(&mut repeat_state.srs_state, grade.max(3));
                    (
                        serde_json::to_string(&repeat_state)?,
                        "active".to_string(),
                        next_review,
                    )
                }
                "memorize" => {
                    let mut repeat_state: domain::review::RepeatableState =
                        serde_json::from_str(&row.state_data).map_err(|_| {
                            AppError::Json(serde_json::Error::io(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid repeatable state data",
                            )))
                        })?;
                    repeat_state.repeats += 1;
                    let next_review =
                        domain::review::next_sm2_review(&mut repeat_state.srs_state, 5);
                    (
                        serde_json::to_string(&repeat_state)?,
                        "active".to_string(),
                        next_review,
                    )
                }
                "dismiss" => (
                    row.state_data,
                    "dismissed".to_string(),
                    Utc::now() + Duration::days(36500),
                ),
                _ => {
                    let mut repeat_state: domain::review::RepeatableState =
                        serde_json::from_str(&row.state_data).map_err(|_| {
                            AppError::Json(serde_json::Error::io(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid repeatable state data",
                            )))
                        })?;
                    repeat_state.repeats += 1;
                    let next_review = domain::review::next_sm2_review(
                        &mut repeat_state.srs_state,
                        if grade == 0 { 1 } else { grade.min(2) },
                    );
                    (
                        serde_json::to_string(&repeat_state)?,
                        "active".to_string(),
                        next_review,
                    )
                }
            };

            reviews::update_queue_state(&self.pool, card_id, new_state_json, status, next_review)
                .await?;
            return Ok(());
        }

        let mut srs_state: SrsState = serde_json::from_str(&row.state_data).map_err(|_| {
            AppError::Json(serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid state data",
            )))
        })?;
        let next_review = domain::review::next_sm2_review(&mut srs_state, grade);
        let new_state_json = serde_json::to_string(&srs_state)?;
        reviews::update_srs_state(&self.pool, card_id, new_state_json, next_review).await
    }
}
