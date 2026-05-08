use serde::{Deserialize, Deserializer, Serialize};

use crate::scheduling::{self, SchedulingState};

#[derive(Serialize, Default)]
pub struct RepeatableState {
    #[serde(default)]
    pub repeats: u32,
    #[serde(default)]
    #[serde(alias = "srs_state")]
    pub scheduling_state: SchedulingState,
}

impl<'de> Deserialize<'de> for RepeatableState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        if value.get("scheduling_state").is_some()
            || value.get("srs_state").is_some()
            || value.get("repeats").is_some()
        {
            #[derive(Deserialize)]
            struct CurrentState {
                #[serde(default)]
                repeats: u32,
                #[serde(default)]
                #[serde(alias = "srs_state")]
                scheduling_state: SchedulingState,
            }

            let current = CurrentState::deserialize(value).map_err(serde::de::Error::custom)?;
            return Ok(Self {
                repeats: current.repeats,
                scheduling_state: current.scheduling_state,
            });
        }

        let scheduling_state =
            SchedulingState::deserialize(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            repeats: 0,
            scheduling_state,
        })
    }
}

pub fn next_review(state: &mut SchedulingState, grade: u8) -> chrono::DateTime<chrono::Utc> {
    scheduling::calculate_next_review(state, grade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_top_level_scheduling_state() {
        let state: RepeatableState = serde_json::from_str(
            r#"{"algorithm":"SM2","ease_factor":2.1,"interval":6,"repetitions":2}"#,
        )
        .unwrap();

        assert_eq!(state.repeats, 0);
        assert_eq!(state.scheduling_state.data.interval, 6);
        assert_eq!(state.scheduling_state.data.repetitions, 2);
    }

    #[test]
    fn deserializes_old_srs_state_field_name() {
        let state: RepeatableState = serde_json::from_str(
            r#"{"repeats":3,"srs_state":{"algorithm":"SM2","ease_factor":2.1,"interval":6,"repetitions":2}}"#,
        )
        .unwrap();

        assert_eq!(state.repeats, 3);
        assert_eq!(state.scheduling_state.data.interval, 6);
    }
}
