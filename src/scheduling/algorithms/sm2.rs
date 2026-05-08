use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Sm2State {
    pub ease_factor: f32,
    pub interval: u32,
    pub repetitions: u32,
}

impl Default for Sm2State {
    fn default() -> Self {
        Self {
            ease_factor: 2.5,
            interval: 0,
            repetitions: 0,
        }
    }
}

pub fn calculate_next_review(state: &mut Sm2State, grade: u8) -> DateTime<Utc> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduling::{Algorithm, SchedulingState};

    #[test]
    fn test_sm2_first_review_pass() {
        let mut state = SchedulingState::default();
        let _ = crate::scheduling::calculate_next_review(&mut state, 5);
        assert_eq!(state.data.interval, 1);
        assert_eq!(state.data.repetitions, 1);
        assert!(state.data.ease_factor > 2.5);
    }

    #[test]
    fn test_sm2_second_review_sets_6_day_interval() {
        let mut state = Sm2State::default();
        let _ = calculate_next_review(&mut state, 4);
        assert_eq!(state.interval, 1);
        let _ = calculate_next_review(&mut state, 4);
        assert_eq!(state.interval, 6);
        assert_eq!(state.repetitions, 2);
    }

    #[test]
    fn test_sm2_third_review_uses_ease_factor() {
        let mut state = Sm2State::default();
        let _ = calculate_next_review(&mut state, 4);
        let _ = calculate_next_review(&mut state, 4);
        let ef_before = state.ease_factor;
        let _ = calculate_next_review(&mut state, 4);
        assert_eq!(state.interval, (6.0 * ef_before).round() as u32);
        assert_eq!(state.repetitions, 3);
    }

    #[test]
    fn test_sm2_fail_resets_reps() {
        let mut state = Sm2State::default();
        let _ = calculate_next_review(&mut state, 4);
        let _ = calculate_next_review(&mut state, 4);
        assert_eq!(state.repetitions, 2);

        let _ = calculate_next_review(&mut state, 2);
        assert_eq!(state.interval, 1);
        assert_eq!(state.repetitions, 0);
        assert!(state.ease_factor < 2.5);
    }

    #[test]
    fn test_sm2_ease_factor_floor() {
        let mut state = Sm2State::default();
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
        let mut state = Sm2State::default();
        let _ = calculate_next_review(&mut state, 3);
        assert_eq!(state.repetitions, 1, "Grade 3 should still count as a pass");
        assert_eq!(state.interval, 1);
    }

    #[test]
    fn test_sm2_grade_boundary_2_is_fail() {
        let mut state = Sm2State::default();
        let _ = calculate_next_review(&mut state, 5);
        assert_eq!(state.repetitions, 1);
        let _ = calculate_next_review(&mut state, 2);
        assert_eq!(state.repetitions, 0, "Grade 2 should reset repetitions");
    }

    #[test]
    fn test_default_state() {
        let state = SchedulingState::default();
        assert!(matches!(state.algorithm, Algorithm::SM2));
        assert_eq!(state.data.ease_factor, 2.5);
        assert_eq!(state.data.interval, 0);
        assert_eq!(state.data.repetitions, 0);
    }
}
