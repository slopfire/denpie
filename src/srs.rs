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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sm2_first_review_pass() {
        let mut state = SrsState::default();
        let _ = calculate_next_review(&mut state, 5); // grade 5 increases ease factor
        assert_eq!(state.interval, 1);
        assert_eq!(state.repetitions, 1);
        assert!(state.ease_factor > 2.5);
    }

    #[test]
    fn test_sm2_second_review_sets_6_day_interval() {
        let mut state = SrsState::default();
        let _ = calculate_next_review(&mut state, 4);
        assert_eq!(state.interval, 1);
        let _ = calculate_next_review(&mut state, 4);
        assert_eq!(state.interval, 6);
        assert_eq!(state.repetitions, 2);
    }

    #[test]
    fn test_sm2_third_review_uses_ease_factor() {
        let mut state = SrsState::default();
        let _ = calculate_next_review(&mut state, 4);
        let _ = calculate_next_review(&mut state, 4);
        let ef_before = state.ease_factor;
        let _ = calculate_next_review(&mut state, 4);
        // interval = round(6 * ef_before)
        assert_eq!(state.interval, (6.0 * ef_before).round() as u32);
        assert_eq!(state.repetitions, 3);
    }

    #[test]
    fn test_sm2_fail_resets_reps() {
        let mut state = SrsState::default();
        let _ = calculate_next_review(&mut state, 4); // rep 1
        let _ = calculate_next_review(&mut state, 4); // rep 2
        assert_eq!(state.repetitions, 2);
        
        let _ = calculate_next_review(&mut state, 2); // fail
        assert_eq!(state.interval, 1);
        assert_eq!(state.repetitions, 0);
        assert!(state.ease_factor < 2.5); // ease factor decreases
    }

    #[test]
    fn test_sm2_ease_factor_floor() {
        let mut state = SrsState::default();
        // Repeatedly fail to drive ease factor down
        for _ in 0..20 {
            let _ = calculate_next_review(&mut state, 0);
        }
        assert!(
            state.ease_factor >= 1.3,
            "Ease factor should never go below 1.3, got {}",
            state.ease_factor
        );
    }

    #[test]
    fn test_sm2_grade_boundary_3_still_passes() {
        let mut state = SrsState::default();
        let _ = calculate_next_review(&mut state, 3);
        assert_eq!(state.repetitions, 1, "Grade 3 should still count as a pass");
        assert_eq!(state.interval, 1);
    }

    #[test]
    fn test_sm2_grade_boundary_2_is_fail() {
        let mut state = SrsState::default();
        // First pass
        let _ = calculate_next_review(&mut state, 5);
        assert_eq!(state.repetitions, 1);
        // Then fail
        let _ = calculate_next_review(&mut state, 2);
        assert_eq!(state.repetitions, 0, "Grade 2 should reset repetitions");
    }

    #[test]
    fn test_fsrs_placeholder_returns_future_date() {
        let mut state = SrsState {
            algorithm: Algorithm::FSRS,
            ease_factor: 2.5,
            interval: 0,
            repetitions: 0,
        };
        let before = Utc::now();
        let next = calculate_next_review(&mut state, 4);
        assert!(next > before, "FSRS should return a future date");
    }

    #[test]
    fn test_default_state() {
        let state = SrsState::default();
        assert!(matches!(state.algorithm, Algorithm::SM2));
        assert_eq!(state.ease_factor, 2.5);
        assert_eq!(state.interval, 0);
        assert_eq!(state.repetitions, 0);
    }
}

