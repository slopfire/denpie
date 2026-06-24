use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tower_sessions::Session;

use crate::AppState;
use crate::dashboard::response::TriggerAutoupdateRes;
use crate::dashboard::util::require_admin;
use crate::services::autoupdate::{AutoupdateService, UpdateStatus};

pub async fn trigger_autoupdate(State(state): State<Arc<AppState>>, session: Session) -> Response {
    match require_admin(&state, &session).await {
        Ok(_) => {}
        Err(err) => return err.into_response(),
    }
    match AutoupdateService::trigger_manual(&state.settings_path).await {
        Ok(result) => {
            if result.should_exit_for_restart {
                tokio::spawn(async {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    std::process::exit(75);
                });
            }
            Json(TriggerAutoupdateRes {
                message: result.message,
                restarting: result.should_exit_for_restart,
                updating: result.update_started,
                target_sha: result.target_sha,
                build_sha: option_env!("DENPIE_BUILD_SHA")
                    .unwrap_or("unknown")
                    .to_string(),
            })
            .into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err).into_response(),
    }
}

pub async fn autoupdate_status(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<UpdateStatus> {
    match require_admin(&state, &session).await {
        Ok(_) => {}
        _ => {
            return Json(UpdateStatus {
                phase: "forbidden".to_string(),
                message: "Admin only".to_string(),
                target_sha: String::new(),
                updated_at: String::new(),
            });
        }
    }
    Json(AutoupdateService::read_status(&state.settings_path))
}
