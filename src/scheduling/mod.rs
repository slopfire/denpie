pub mod algorithms;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Algorithm {
    #[serde(rename = "SM2", alias = "FSRS", alias = "fsrs")]
    SM2,
}

impl Algorithm {
    pub fn storage_name(&self) -> &'static str {
        match self {
            Self::SM2 => "sm2",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SchedulingState {
    pub algorithm: Algorithm,
    #[serde(flatten)]
    pub data: algorithms::sm2::Sm2State,
}

impl Default for SchedulingState {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::SM2,
            data: algorithms::sm2::Sm2State::default(),
        }
    }
}

pub fn calculate_next_review(state: &mut SchedulingState, grade: u8) -> DateTime<Utc> {
    match state.algorithm {
        Algorithm::SM2 => algorithms::sm2::calculate_next_review(&mut state.data, grade),
    }
}
