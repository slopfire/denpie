use axum::{body::Bytes, extract::State, http::StatusCode, response::Response};
use prost::Message;
use std::sync::Arc;

use crate::AppState;

use super::{
    admin::{app_summary_pb, app_topics_pb, list_admin_topics_pb, list_tipcards_pb},
    auth::{create_raw_api_key, delete_api_key_by_id, list_api_keys_pb, require_api_key},
    pb,
    response::{empty_response, protobuf_response, tip_to_pb},
    reviews::apply_review,
    settings::{current_settings, update_settings_file},
    tipcards::{delete_tipcard_by_id, set_tipcard_pinned},
    tips::{build_tips, create_custom_tipcard, force_daily_refresh},
    topics::{delete_topic_by_id, update_topic_prompt},
    types::{ForceDailyRefreshRequest, TipsJsonRequest},
};

pub async fn unified_api(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let request =
        pb::ApiRequest::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let op = request
        .op
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing API operation".to_string()))?;

    let response = match op {
        pb::api_request::Op::BootstrapApiKey(req) => {
            let settings = state
                .settings
                .get_settings()
                .map_err(|err| err.into_status_body())?;
            if settings.admin_token.is_empty() || req.admin_token != settings.admin_token {
                return Err((StatusCode::UNAUTHORIZED, "Invalid admin token".to_string()));
            }
            let admin = crate::db::repositories::users::first_admin(&state.db)
                .await
                .map_err(|err| err.into_status_body())?;
            let user_id = if let Some(admin) = admin {
                admin.id
            } else {
                return Err((
                    StatusCode::CONFLICT,
                    "Setup required before bootstrapping API keys".to_string(),
                ));
            };
            let api_key = create_raw_api_key(&state, &user_id, Some(req.client_name)).await?;
            pb::ApiResponse {
                result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                    api_key,
                })),
            }
        }
        other => {
            let user = require_api_key(&state, &request.auth).await?;
            handle_authenticated_op(&state, &user, other).await?
        }
    };

    Ok(protobuf_response(&response))
}

async fn handle_authenticated_op(
    state: &AppState,
    user: &crate::auth::AuthUser,
    op: pb::api_request::Op,
) -> Result<pb::ApiResponse, (StatusCode, String)> {
    match op {
        pb::api_request::Op::Tips(query) => {
            let responses = build_tips(
                state,
                &user.id,
                TipsJsonRequest {
                    count: Some(query.count as u32),
                    topics: query.topics,
                    tipcard_type: Some(query.tipcard_type),
                    exclude_card_ids: Some(query.exclude_card_ids),
                    manual_content: Some(query.manual_content),
                    manual_compressed_content: Some(query.manual_compressed_content),
                    manual_image_data: None,
                },
            )
            .await?
            .into_iter()
            .map(tip_to_pb)
            .collect();
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                    tips: responses,
                })),
            })
        }
        pb::api_request::Op::SubmitCustomTipcard(req) => {
            let card = create_custom_tipcard(state, &user.id, req).await?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::Tips(pb::TipsResponse {
                    tips: vec![tip_to_pb(card)],
                })),
            })
        }
        pb::api_request::Op::ForceDailyRefresh(req) => {
            let result = force_daily_refresh(
                state,
                &user.id,
                ForceDailyRefreshRequest {
                    topics: req.topics,
                    tipcard_type: Some(req.tipcard_type),
                },
            )
            .await?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::ForceDailyRefresh(
                    pb::ForceDailyRefreshResponse {
                        refreshed_cards: result.refreshed_cards,
                    },
                )),
            })
        }
        pb::api_request::Op::Review(payload) => {
            apply_review(
                state,
                &user.id,
                payload.card_id,
                payload.grade as u8,
                &payload.action,
            )
            .await?;
            Ok(empty_response())
        }
        pb::api_request::Op::GetTopics(_) => {
            let rows = crate::db::repositories::topics::list_names(&state.db, &user.id)
                .await
                .map_err(|err| err.into_status_body())?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::Topics(pb::GetTopicsResponse {
                    topics: rows,
                })),
            })
        }
        pb::api_request::Op::GetSettings(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::Settings(
                current_settings(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::UpdateSettings(req) => {
            update_settings_file(state, &user.id, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::CreateApiKey(req) => {
            let api_key = create_raw_api_key(state, &user.id, Some(req.client_name)).await?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                    api_key,
                })),
            })
        }
        pb::api_request::Op::ListApiKeys(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::ApiKeys(
                list_api_keys_pb(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::DeleteApiKey(req) => {
            delete_api_key_by_id(state, &user.id, req.id).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::ListAdminTopics(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::AdminTopics(
                list_admin_topics_pb(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::ListTipcards(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::Tipcards(
                list_tipcards_pb(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::DeleteTipcard(req) => {
            delete_tipcard_by_id(state, &user.id, req.id).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::PinTipcard(req) => {
            set_tipcard_pinned(state, &user.id, req.id, req.pinned).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::DeleteTopic(req) => {
            delete_topic_by_id(state, &user.id, req.id).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::GetSummary(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::Summary(
                app_summary_pb(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::ListAppTopics(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::AppTopics(
                app_topics_pb(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::UpdateTopic(req) => {
            update_topic_prompt(state, &user.id, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::BootstrapApiKey(_) => unreachable!(),
    }
}
