use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum Algorithm {
    FSRS,
    SM2,
}

#[derive(Serialize, Deserialize)]
pub struct SrsState {
    pub algorithm: Algorithm,
    // For SM2
    pub ease_factor: f32,
    pub interval: u32,
    pub repetitions: u32,
    // FSRS state would go here
}

impl Default for SrsState {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::SM2,
            ease_factor: 2.5,
            interval: 0,
            repetitions: 0,
        }
    }
}

pub fn calculate_next_review(state: &mut SrsState, grade: u8) -> DateTime<Utc> {
    match state.algorithm {
        Algorithm::SM2 => {
            if grade >= 3 {
                if state.repetitions == 0 {
                    state.interval = 1;
                } else if state.repetitions == 1 {
                    state.interval = 6;
                } else {
                    state.interval = (state.interval as f32 * state.ease_factor).round() as u32;
                }
                state.repetitions += 1;
            } else {
                state.repetitions = 0;
                state.interval = 1;
            }

            state.ease_factor += 0.1 - (5.0 - grade as f32) * (0.08 + (5.0 - grade as f32) * 0.02);
            if state.ease_factor < 1.3 {
                state.ease_factor = 1.3;
            }

            Utc::now() + chrono::Duration::days(state.interval as i64)
        }
        Algorithm::FSRS => {
            // Placeholder for FSRS logic using fsrs crate
            Utc::now() + chrono::Duration::days(1)
        }
    }
}
