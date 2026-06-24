use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use tower_sessions::Session;

use crate::AppState;
use crate::dashboard::response::TokenSpend;
use crate::dashboard::util::{current_user, optional_user};
use crate::services::tips::TipService;
use crate::types::{
    ForceDailyRefreshRequest, ForceDailyRefreshResponse, ReviewJsonRequest, TipCardJson,
    TipsJsonRequest,
};

pub async fn token_spend(State(state): State<Arc<AppState>>, session: Session) -> Json<TokenSpend> {
    let user = match optional_user(&state, &session).await {
        Some(user) => user,
        None => {
            return Json(TokenSpend {
                daily: 0,
                monthly: 0,
                total: 0,
            });
        }
    };
    match TipService::aggregate_token_spend(&state, &user.id).await {
        Ok(spend) => Json(TokenSpend {
            daily: spend.daily,
            monthly: spend.monthly,
            total: spend.total,
        }),
        Err(_) => Json(TokenSpend {
            daily: 0,
            monthly: 0,
            total: 0,
        }),
    }
}

pub async fn app_tips(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<TipsJsonRequest>,
) -> Result<Json<Vec<TipCardJson>>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    TipService::build_tips(&state, &user.id, req)
        .await
        .map(Json)
}

pub async fn app_review(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<ReviewJsonRequest>,
) -> Result<Json<()>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let grade = req.grade.unwrap_or(3).min(5);
    let action = req.action.unwrap_or_default();
    state
        .reviews
        .apply_review(&user.id, req.card_id, grade, &action)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(()))
}

pub async fn force_daily_refresh(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<ForceDailyRefreshRequest>,
) -> Result<Json<ForceDailyRefreshResponse>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    TipService::force_daily_refresh(&state, &user.id, req)
        .await
        .map(Json)
}
