use serde::{Deserialize, Serialize};

use crate::srs::{self, SrsState};

#[derive(Serialize, Deserialize, Default)]
pub struct RepeatableState {
    pub repeats: u32,
    #[serde(default)]
    pub srs_state: SrsState,
}

pub fn next_sm2_review(state: &mut SrsState, grade: u8) -> chrono::DateTime<chrono::Utc> {
    srs::calculate_next_review(state, grade)
}
