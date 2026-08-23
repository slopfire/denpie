use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use prost::Message;
use rand::{Rng, distributions::Alphanumeric};
use sha2::{Digest, Sha256};
use std::{future::Future, sync::Arc, time::Duration};
use tracing::Instrument;

use crate::{AppState, db::repositories::api_idempotency};

use super::{
    admin::{app_summary_pb, app_topics_pb, list_admin_topics_pb, list_tipcards_pb},
    auth::{
        create_raw_api_key, create_scoped_api_key, delete_api_key_by_id, list_api_keys_pb,
        request_api_key, require_api_key, resolve_principal,
    },
    contract,
    documents::{
        add_document, add_pool_image, attach_document_topic, delete_document, delete_pool_image,
        detach_document_topic, list_documents, list_pool_images,
    },
    pb, resources,
    response::{empty_response, protobuf_response, protobuf_response_with_status, tip_to_pb},
    reviews::apply_review,
    settings::{current_settings, update_settings_file},
    tipcards::{
        append_tipcard_images, delete_tipcard_by_id, set_tipcard_images, set_tipcard_pinned,
    },
    tips::{build_tips, create_custom_tipcard, force_daily_refresh},
    topics::{delete_topic_by_id, update_topic_prompt},
    types::{ForceDailyRefreshOutcome, ForceDailyRefreshRequest, TipsJsonRequest},
};

pub async fn unified_api(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, (StatusCode, String)> {
    let request =
        pb::ApiRequest::decode(body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let response = execute_request(&state, &headers, request).await?;
    Ok(protobuf_response(&response))
}

pub async fn api_v1(
    State(state): State<Arc<AppState>>,
    session: tower_sessions::Session,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let fallback_request_id = generate_request_id();
    if !is_protobuf_content_type(&headers) {
        return v1_error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            fallback_request_id,
            "Content-Type must be application/x-protobuf or application/protobuf".to_string(),
        );
    }

    let envelope = match pb::ApiV1Request::decode(body) {
        Ok(envelope) => envelope,
        Err(err) => {
            return v1_error_response(
                StatusCode::BAD_REQUEST,
                fallback_request_id,
                format!("Invalid protobuf request: {err}"),
            );
        }
    };
    let request_id = if envelope.request_id.is_empty() {
        fallback_request_id
    } else if valid_request_id(&envelope.request_id) {
        envelope.request_id
    } else {
        return v1_error_response(
            StatusCode::BAD_REQUEST,
            fallback_request_id,
            "request_id must be 1-64 ASCII letters, digits, '.', '_' or '-'".to_string(),
        );
    };
    let Some(call) = envelope.call else {
        return v1_error_response(
            StatusCode::BAD_REQUEST,
            request_id,
            "Missing API call".to_string(),
        );
    };
    let Some(op) = call.op.as_ref() else {
        return v1_error_response(
            StatusCode::BAD_REQUEST,
            request_id,
            "Missing API operation".to_string(),
        );
    };
    let policy = mutation_policy(op);
    let idempotency_key = match resolve_idempotency_key(
        &headers,
        &envelope.idempotency_key,
        policy != MutationPolicy::ReadOnly,
    ) {
        Ok(key) => key,
        Err((status, message)) => return v1_error_response(status, request_id, message),
    };

    execute_v1_request(
        &state,
        &headers,
        &session,
        call,
        request_id,
        idempotency_key,
        policy,
    )
    .await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MutationPolicy {
    ReadOnly,
    Replayable,
    OneTimeSecret,
}

async fn execute_v1_request(
    state: &AppState,
    headers: &HeaderMap,
    session: &tower_sessions::Session,
    request: pb::ApiRequest,
    request_id: String,
    idempotency_key: Option<String>,
    policy: MutationPolicy,
) -> Response {
    let op = request.op.expect("API v1 operation was validated");
    let request_hash = hash_api_operation(&op);
    let op_name = api_op_name(&op);
    let expected_result = contract::expected_result_field(&op);

    let response = match op {
        pb::api_request::Op::BootstrapApiKey(req) => {
            let user_id = match authorize_bootstrap(state, &req.admin_token).await {
                Ok(user_id) => user_id,
                Err((status, message)) => {
                    return v1_error_response(status, request_id, message);
                }
            };
            let actor_id = format!("bootstrap:{user_id}");
            let operation_user_id = user_id.clone();
            run_idempotent_call(
                state,
                &actor_id,
                &user_id,
                idempotency_key
                    .as_deref()
                    .expect("mutation key was validated"),
                &request_hash,
                policy,
                request_id,
                async move {
                    let response =
                        create_bootstrap_key(state, &operation_user_id, req.client_name).await?;
                    validate_operation_result(expected_result, response)
                },
            )
            .await
        }
        pb::api_request::Op::GetApiInfo(_) => {
            let result = validate_operation_result(
                expected_result,
                pb::ApiResponse {
                    result: Some(pb::api_response::Result::ApiInfo(resources::api_info())),
                },
            );
            v1_result_response(request_id, result)
        }
        other => {
            let principal = match resolve_principal(state, headers, &request.auth, session).await {
                Ok(principal) => principal,
                Err((status, message)) => {
                    return v1_error_response(status, request_id, message);
                }
            };
            if let Err((status, message)) = require_scope(&principal, required_scope(&other)) {
                return v1_error_response(status, request_id, message);
            }

            if policy == MutationPolicy::ReadOnly {
                v1_result_response(
                    request_id,
                    handle_authenticated_op_checked(state, &principal, other, expected_result)
                        .await,
                )
            } else {
                let actor_id = principal.idempotency_actor_id();
                let user_id = principal.user.id.clone();
                run_idempotent_call(
                    state,
                    &actor_id,
                    &user_id,
                    idempotency_key
                        .as_deref()
                        .expect("mutation key was validated"),
                    &request_hash,
                    policy,
                    request_id,
                    handle_authenticated_op_checked(state, &principal, other, expected_result),
                )
                .await
            }
        }
    };

    tracing::debug!(op = op_name, "completed API v1 request");
    response
}

#[allow(clippy::too_many_arguments)]
async fn run_idempotent_call<F>(
    state: &AppState,
    actor_id: &str,
    user_id: &str,
    idempotency_key: &str,
    request_hash: &str,
    policy: MutationPolicy,
    request_id: String,
    operation: F,
) -> Response
where
    F: Future<Output = Result<pb::ApiResponse, (StatusCode, String)>>,
{
    let claim =
        match api_idempotency::claim(&state.db, actor_id, user_id, idempotency_key, request_hash)
            .await
        {
            Ok(claim) => claim,
            Err(err) => {
                let (status, message) = err.into_status_body();
                return v1_error_response(status, request_id, message);
            }
        };

    match claim {
        api_idempotency::IdempotencyRecord::Acquired => {}
        api_idempotency::IdempotencyRecord::Completed {
            status_code,
            response_body,
        } => {
            return replay_idempotent_response(
                status_code,
                response_body,
                request_id,
                idempotency_key,
            );
        }
        api_idempotency::IdempotencyRecord::Conflict => {
            return idempotency_error_response(
                StatusCode::CONFLICT,
                request_id,
                idempotency_key,
                "Idempotency key was already used with a different request".to_string(),
                false,
            );
        }
        api_idempotency::IdempotencyRecord::InProgress { created_at } => {
            if let Some(response) = wait_for_idempotent_result(
                state,
                actor_id,
                idempotency_key,
                request_hash,
                &request_id,
            )
            .await
            {
                return response;
            }
            let recently_started = chrono::Utc::now().signed_duration_since(created_at)
                <= chrono::Duration::minutes(5);
            let message = if recently_started {
                "An identical request is still in progress; retry with the same idempotency key"
            } else {
                "The original mutation has an indeterminate outcome; this idempotency key remains locked to prevent duplicate execution"
            };
            let mut response = idempotency_error_response(
                StatusCode::CONFLICT,
                request_id,
                idempotency_key,
                message.to_string(),
                recently_started,
            );
            if recently_started {
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
            }
            return response;
        }
    }

    let (status, mut stored_response) = v1_message_from_result(String::new(), operation.await);
    let encoded = if policy == MutationPolicy::OneTimeSecret && status.is_success() {
        None
    } else {
        Some(stored_response.encode_to_vec())
    };
    if let Err(err) = api_idempotency::complete(
        &state.db,
        actor_id,
        idempotency_key,
        request_hash,
        status.as_u16(),
        encoded.as_deref(),
    )
    .await
    {
        tracing::error!(
            error = ?err,
            actor_id,
            idempotency_key,
            "API mutation completed but its idempotency result could not be persisted"
        );
        return idempotency_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            request_id,
            idempotency_key,
            "Mutation outcome could not be recorded; retry only with the same idempotency key"
                .to_string(),
            true,
        );
    }

    stored_response.request_id = request_id;
    let mut response = protobuf_response_with_status(status, &stored_response);
    insert_request_id_header(&mut response, &stored_response.request_id);
    insert_idempotency_headers(&mut response, idempotency_key, false);
    response
}

async fn wait_for_idempotent_result(
    state: &AppState,
    actor_id: &str,
    idempotency_key: &str,
    request_hash: &str,
    request_id: &str,
) -> Option<Response> {
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        match api_idempotency::lookup(&state.db, actor_id, idempotency_key, request_hash).await {
            Ok(api_idempotency::IdempotencyRecord::Completed {
                status_code,
                response_body,
            }) => {
                return Some(replay_idempotent_response(
                    status_code,
                    response_body,
                    request_id.to_string(),
                    idempotency_key,
                ));
            }
            Ok(api_idempotency::IdempotencyRecord::Conflict) => {
                return Some(idempotency_error_response(
                    StatusCode::CONFLICT,
                    request_id.to_string(),
                    idempotency_key,
                    "Idempotency key was already used with a different request".to_string(),
                    false,
                ));
            }
            Ok(api_idempotency::IdempotencyRecord::InProgress { .. }) => {}
            Ok(api_idempotency::IdempotencyRecord::Acquired) => unreachable!(),
            Err(err) => {
                tracing::error!(error = ?err, "failed to poll API idempotency result");
                return Some(idempotency_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    request_id.to_string(),
                    idempotency_key,
                    "Could not read idempotency result".to_string(),
                    true,
                ));
            }
        }
    }
    None
}

async fn execute_request(
    state: &AppState,
    headers: &HeaderMap,
    request: pb::ApiRequest,
) -> Result<pb::ApiResponse, (StatusCode, String)> {
    let op = request
        .op
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing API operation".to_string()))?;
    let op_name = api_op_name(&op);
    let expected_result = contract::expected_result_field(&op);

    async {
        let response = match op {
            pb::api_request::Op::BootstrapApiKey(req) => {
                let user_id = authorize_bootstrap(state, &req.admin_token).await?;
                create_bootstrap_key(state, &user_id, req.client_name).await?
            }
            pb::api_request::Op::GetApiInfo(_) => pb::ApiResponse {
                result: Some(pb::api_response::Result::ApiInfo(resources::api_info())),
            },
            other => {
                let api_key = request_api_key(headers, &request.auth)?;
                let user = require_api_key(state, &api_key).await?;
                handle_authenticated_op(state, &user, other).await?
            }
        };
        validate_operation_result(expected_result, response)
    }
    .instrument(tracing::info_span!("api_request", op = op_name))
    .await
}

async fn authorize_bootstrap(
    state: &AppState,
    admin_token: &str,
) -> Result<String, (StatusCode, String)> {
    let settings = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    if settings.admin_token.is_empty() || admin_token != settings.admin_token {
        return Err((StatusCode::UNAUTHORIZED, "Invalid admin token".to_string()));
    }
    crate::db::repositories::users::first_admin(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
        .map(|admin| admin.id)
        .ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "Setup required before bootstrapping API keys".to_string(),
            )
        })
}

async fn create_bootstrap_key(
    state: &AppState,
    user_id: &str,
    client_name: String,
) -> Result<pb::ApiResponse, (StatusCode, String)> {
    let api_key = create_raw_api_key(state, user_id, Some(client_name)).await?;
    Ok(pb::ApiResponse {
        result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
            api_key,
        })),
    })
}

fn resolve_idempotency_key(
    headers: &HeaderMap,
    envelope_key: &str,
    required: bool,
) -> Result<Option<String>, (StatusCode, String)> {
    let header_key = headers
        .get("idempotency-key")
        .map(|value| {
            value.to_str().map(str::to_string).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "Idempotency-Key must contain valid ASCII".to_string(),
                )
            })
        })
        .transpose()?;
    let envelope_key = if envelope_key.is_empty() {
        None
    } else {
        Some(envelope_key.to_string())
    };
    if let (Some(header), Some(envelope)) = (&header_key, &envelope_key)
        && header != envelope
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Idempotency-Key header and protobuf idempotency_key must match".to_string(),
        ));
    }
    let key = header_key.or(envelope_key);
    if required && key.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Mutating API v1 operations require an Idempotency-Key header or protobuf idempotency_key"
                .to_string(),
        ));
    }
    if let Some(key) = &key
        && !valid_idempotency_key(key)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "idempotency key must be 1-128 ASCII letters, digits, '.', '_', ':', or '-'"
                .to_string(),
        ));
    }
    Ok(key)
}

fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn hash_api_operation(op: &pb::api_request::Op) -> String {
    let request = pb::ApiRequest {
        auth: String::new(),
        op: Some(op.clone()),
    };
    let mut hasher = Sha256::new();
    hasher.update(request.encode_to_vec());
    hex::encode(hasher.finalize())
}

fn mutation_policy(op: &pb::api_request::Op) -> MutationPolicy {
    match op {
        pb::api_request::Op::GetApiInfo(_)
        | pb::api_request::Op::GetTopics(_)
        | pb::api_request::Op::GetSettings(_)
        | pb::api_request::Op::ListApiKeys(_)
        | pb::api_request::Op::ListAdminTopics(_)
        | pb::api_request::Op::ListTipcards(_)
        | pb::api_request::Op::GetSummary(_)
        | pb::api_request::Op::ListAppTopics(_)
        | pb::api_request::Op::ListDocuments(_)
        | pb::api_request::Op::ListPoolImages(_)
        | pb::api_request::Op::ListFlowCards(_)
        | pb::api_request::Op::GetTipcard(_)
        | pb::api_request::Op::GetDocument(_)
        | pb::api_request::Op::ExploreLink(_)
        | pb::api_request::Op::TestVisionModel(_) => MutationPolicy::ReadOnly,
        pb::api_request::Op::BootstrapApiKey(_)
        | pb::api_request::Op::CreateApiKey(_)
        | pb::api_request::Op::CreateApiKeyV1(_) => MutationPolicy::OneTimeSecret,
        pb::api_request::Op::Tips(_)
        | pb::api_request::Op::SubmitCustomTipcard(_)
        | pb::api_request::Op::ForceDailyRefresh(_)
        | pb::api_request::Op::Review(_)
        | pb::api_request::Op::UpdateSettings(_)
        | pb::api_request::Op::DeleteApiKey(_)
        | pb::api_request::Op::DeleteTipcard(_)
        | pb::api_request::Op::PinTipcard(_)
        | pb::api_request::Op::AppendTipcardImages(_)
        | pb::api_request::Op::ReplaceTipcardImages(_)
        | pb::api_request::Op::UpdateTopic(_)
        | pb::api_request::Op::DeleteTopic(_)
        | pb::api_request::Op::AddDocument(_)
        | pb::api_request::Op::DeleteDocument(_)
        | pb::api_request::Op::AddPoolImage(_)
        | pb::api_request::Op::DeletePoolImage(_)
        | pb::api_request::Op::AttachDocumentTopic(_)
        | pb::api_request::Op::DetachDocumentTopic(_)
        | pb::api_request::Op::ContinueDailyReview(_)
        | pb::api_request::Op::CreateDocument(_)
        | pb::api_request::Op::UploadDocument(_)
        | pb::api_request::Op::CreatePoolImage(_)
        | pb::api_request::Op::TipsV1(_)
        | pb::api_request::Op::ReviewV1(_)
        | pb::api_request::Op::ReviewAndAdvance(_) => MutationPolicy::Replayable,
    }
}

fn is_protobuf_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("application/x-protobuf")
                || value.eq_ignore_ascii_case("application/protobuf")
        })
}

fn generate_request_id() -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect();
    format!("req_{suffix}")
}

fn valid_request_id(request_id: &str) -> bool {
    !request_id.is_empty()
        && request_id.len() <= 64
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn v1_error_response(status: StatusCode, request_id: String, message: String) -> Response {
    let (status, response) = v1_message_from_result(request_id.clone(), Err((status, message)));
    let mut response = protobuf_response_with_status(status, &response);
    insert_request_id_header(&mut response, &request_id);
    response
}

/// Replace a middleware/proxy plain-text or HTML error with the documented
/// `ApiV1Response.error` envelope so the browser never tries to decode
/// `"Too Many Requests"` or `<!DOCTYPE html>` as protobuf.
pub async fn replace_non_protobuf_v1_error(response: Response) -> Response {
    if response.status().is_success() || is_protobuf_content_type(response.headers()) {
        return response;
    }
    let status = response.status();
    let retry_after = response.headers().get(header::RETRY_AFTER).cloned();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = axum::body::to_bytes(response.into_body(), 8 * 1024)
        .await
        .unwrap_or_default();
    v1_error_from_plain_response(status, retry_after, request_id.as_deref(), &body)
}

fn v1_error_from_plain_response(
    status: StatusCode,
    retry_after: Option<HeaderValue>,
    existing_request_id: Option<&str>,
    body: &[u8],
) -> Response {
    let request_id = existing_request_id
        .filter(|id| valid_request_id(id))
        .map(str::to_owned)
        .unwrap_or_else(generate_request_id);
    let mut response = v1_error_response(status, request_id, plain_error_message(status, body));
    if let Some(retry_after) = retry_after {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, retry_after);
    }
    response
}

fn plain_error_message(status: StatusCode, body: &[u8]) -> String {
    if status == StatusCode::TOO_MANY_REQUESTS {
        return "Too many requests; retry shortly".to_string();
    }
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.is_empty() || text.starts_with('<') {
        format!("HTTP {} response was not protobuf", status.as_u16())
    } else {
        text.chars().take(300).collect()
    }
}

fn v1_result_response(
    request_id: String,
    result: Result<pb::ApiResponse, (StatusCode, String)>,
) -> Response {
    let (status, response) = v1_message_from_result(request_id.clone(), result);
    let mut response = protobuf_response_with_status(status, &response);
    insert_request_id_header(&mut response, &request_id);
    response
}

fn v1_message_from_result(
    request_id: String,
    result: Result<pb::ApiResponse, (StatusCode, String)>,
) -> (StatusCode, pb::ApiV1Response) {
    match result {
        Ok(success) => (
            StatusCode::OK,
            pb::ApiV1Response {
                request_id,
                outcome: Some(pb::api_v1_response::Outcome::Success(Box::new(success))),
            },
        ),
        Err((status, message)) => (
            status,
            pb::ApiV1Response {
                request_id,
                outcome: Some(pb::api_v1_response::Outcome::Error(api_error(
                    status, message, None,
                ))),
            },
        ),
    }
}

fn api_error(status: StatusCode, message: String, retryable: Option<bool>) -> pb::ApiError {
    let code = match status {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
            pb::ApiErrorCode::InvalidArgument
        }
        StatusCode::UNAUTHORIZED => pb::ApiErrorCode::Unauthenticated,
        StatusCode::FORBIDDEN => pb::ApiErrorCode::PermissionDenied,
        StatusCode::NOT_FOUND => pb::ApiErrorCode::NotFound,
        StatusCode::CONFLICT => pb::ApiErrorCode::Conflict,
        StatusCode::TOO_MANY_REQUESTS => pb::ApiErrorCode::RateLimited,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => pb::ApiErrorCode::UnsupportedMediaType,
        _ => pb::ApiErrorCode::Internal,
    };
    pb::ApiError {
        code: code as i32,
        message,
        retryable: retryable
            .unwrap_or(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()),
    }
}

fn replay_idempotent_response(
    status_code: u16,
    response_body: Option<Vec<u8>>,
    request_id: String,
    idempotency_key: &str,
) -> Response {
    let Some(response_body) = response_body else {
        return idempotency_error_response(
            StatusCode::CONFLICT,
            request_id,
            idempotency_key,
            "The original request succeeded with a one-time credential that cannot be replayed; revoke it if necessary and use a new idempotency key"
                .to_string(),
            false,
        );
    };
    let status = match StatusCode::from_u16(status_code) {
        Ok(status) => status,
        Err(_) => {
            return idempotency_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                request_id,
                idempotency_key,
                "Stored idempotency result has an invalid HTTP status".to_string(),
                true,
            );
        }
    };
    let mut stored = match pb::ApiV1Response::decode(response_body.as_slice()) {
        Ok(stored) => stored,
        Err(err) => {
            tracing::error!(error = ?err, "stored idempotency response could not be decoded");
            return idempotency_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                request_id,
                idempotency_key,
                "Stored idempotency result is invalid".to_string(),
                true,
            );
        }
    };
    stored.request_id = request_id;
    let mut response = protobuf_response_with_status(status, &stored);
    insert_request_id_header(&mut response, &stored.request_id);
    insert_idempotency_headers(&mut response, idempotency_key, true);
    response
}

fn idempotency_error_response(
    status: StatusCode,
    request_id: String,
    idempotency_key: &str,
    message: String,
    retryable: bool,
) -> Response {
    let response_message = pb::ApiV1Response {
        request_id: request_id.clone(),
        outcome: Some(pb::api_v1_response::Outcome::Error(api_error(
            status,
            message,
            Some(retryable),
        ))),
    };
    let mut response = protobuf_response_with_status(status, &response_message);
    insert_request_id_header(&mut response, &request_id);
    insert_idempotency_headers(&mut response, idempotency_key, false);
    response
}

fn insert_request_id_header(response: &mut Response, request_id: &str) {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
}

fn insert_idempotency_headers(response: &mut Response, idempotency_key: &str, replayed: bool) {
    if let Ok(value) = HeaderValue::from_str(idempotency_key) {
        response.headers_mut().insert("idempotency-key", value);
    }
    if replayed {
        response
            .headers_mut()
            .insert("idempotency-replayed", HeaderValue::from_static("true"));
    }
}

async fn handle_authenticated_op_checked(
    state: &AppState,
    principal: &crate::services::api_keys::ApiPrincipal,
    op: pb::api_request::Op,
    expected_result: &'static str,
) -> Result<pb::ApiResponse, (StatusCode, String)> {
    let response = handle_authenticated_op(state, principal, op).await?;
    validate_operation_result(expected_result, response)
}

fn validate_operation_result(
    expected: &'static str,
    response: pb::ApiResponse,
) -> Result<pb::ApiResponse, (StatusCode, String)> {
    let actual = contract::actual_result_field(&response);
    if actual == Some(expected) {
        return Ok(response);
    }
    tracing::error!(
        expected,
        actual,
        "API operation returned the wrong result variant"
    );
    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal server error".to_string(),
    ))
}

async fn handle_authenticated_op(
    state: &AppState,
    principal: &crate::services::api_keys::ApiPrincipal,
    op: pb::api_request::Op,
) -> Result<pb::ApiResponse, (StatusCode, String)> {
    require_scope(principal, required_scope(&op))?;
    let user = &principal.user;
    match op {
        pb::api_request::Op::Tips(query) => {
            let count = u32::try_from(query.count).map_err(|_| {
                (
                    StatusCode::BAD_REQUEST,
                    "count must fit in an unsigned 32-bit integer".to_string(),
                )
            })?;
            let responses = build_tips(
                state,
                &user.id,
                TipsJsonRequest {
                    count: Some(count),
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
                        outcome: match result.outcome {
                            ForceDailyRefreshOutcome::CardAvailable => {
                                pb::ForceDailyRefreshOutcome::CardAvailable as i32
                            }
                            ForceDailyRefreshOutcome::QueueRefilled => {
                                pb::ForceDailyRefreshOutcome::QueueRefilled as i32
                            }
                            ForceDailyRefreshOutcome::NoChange => {
                                pb::ForceDailyRefreshOutcome::NoChange as i32
                            }
                            ForceDailyRefreshOutcome::ActiveLimitReached => {
                                pb::ForceDailyRefreshOutcome::ActiveLimitReached as i32
                            }
                        },
                        available_cards: result.available_cards,
                        generated_cards: result.generated_cards,
                    },
                )),
            })
        }
        pb::api_request::Op::Review(payload) => {
            let grade = validate_grade(payload.grade)?;
            validate_review_action(&payload.action)?;
            apply_review(state, &user.id, payload.card_id, grade, &payload.action).await?;
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
                current_settings(state, user, principal.has_scope("secrets:read")).await?,
            )),
        }),
        pb::api_request::Op::UpdateSettings(req) => {
            update_settings_file(state, user, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::CreateApiKey(req) => {
            if !principal.can_create_unrestricted_key() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Only a non-expiring full-access key can create legacy full-access keys"
                        .to_string(),
                ));
            }
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
        pb::api_request::Op::AppendTipcardImages(req) => {
            append_tipcard_images(
                state,
                &user.id,
                req.card_id,
                req.image_data,
                req.pool_image_ids,
                req.urls,
            )
            .await?;
            Ok(empty_response())
        }
        pb::api_request::Op::ReplaceTipcardImages(req) => {
            set_tipcard_images(state, &user.id, req.card_id, req.image_data).await?;
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
        pb::api_request::Op::AddDocument(req) => {
            add_document(state, &user.id, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::ListDocuments(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::Documents(
                list_documents(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::DeleteDocument(req) => {
            delete_document(state, &user.id, req.id).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::AttachDocumentTopic(req) => {
            attach_document_topic(state, &user.id, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::DetachDocumentTopic(req) => {
            detach_document_topic(state, &user.id, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::AddPoolImage(req) => {
            add_pool_image(state, &user.id, req).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::ListPoolImages(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::PoolImages(
                list_pool_images(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::DeletePoolImage(req) => {
            delete_pool_image(state, &user.id, req.id).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::CreateDocument(req) => {
            let id = add_document(state, &user.id, req).await?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::DocumentCreated(
                    resources::get_document(state, &user.id, id).await?,
                )),
            })
        }
        pb::api_request::Op::UploadDocument(req) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::DocumentCreated(
                resources::upload_document(state, &user.id, req).await?,
            )),
        }),
        pb::api_request::Op::CreatePoolImage(req) => {
            let result = add_pool_image(state, &user.id, req).await?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::PoolImageCreated(
                    resources::pool_image_created(result),
                )),
            })
        }
        pb::api_request::Op::TipsV1(req) => {
            if req.topics.is_empty() || req.topics.iter().any(|topic| topic.trim().is_empty()) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "topics must contain at least one non-empty topic".to_string(),
                ));
            }
            if req.topics.iter().any(|topic| topic.contains(',')) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "topic names cannot contain commas".to_string(),
                ));
            }
            let tipcard_type = tipcard_type_value(req.tipcard_type, false)?;
            let responses = build_tips(
                state,
                &user.id,
                TipsJsonRequest {
                    count: Some(req.count.max(1)),
                    topics: req.topics.join(","),
                    tipcard_type: Some(tipcard_type.to_string()),
                    exclude_card_ids: Some(req.exclude_card_ids),
                    manual_content: Some(req.manual_content),
                    manual_compressed_content: Some(req.manual_compressed_content),
                    manual_image_data: Some(req.manual_image_data),
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
        pb::api_request::Op::ReviewV1(req) => {
            let grade = validate_grade(req.grade)?;
            let action = review_action_value(req.action)?;
            apply_review(state, &user.id, req.card_id, grade, action).await?;
            Ok(empty_response())
        }
        pb::api_request::Op::ReviewAndAdvance(req) => {
            let grade = validate_grade(req.grade)?;
            let action = review_action_value(req.action)?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::ReviewAndAdvance(
                    resources::review_and_advance(state, &user.id, req, grade, action).await?,
                )),
            })
        }
        pb::api_request::Op::CreateApiKeyV1(req) => {
            let api_key = create_scoped_api_key(state, principal, req).await?;
            Ok(pb::ApiResponse {
                result: Some(pb::api_response::Result::ApiKeyCreated(pb::ApiKeyCreated {
                    api_key,
                })),
            })
        }
        pb::api_request::Op::ListFlowCards(req) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::FlowCardPage(
                resources::list_flow_cards(state, &user.id, req).await?,
            )),
        }),
        pb::api_request::Op::GetTipcard(req) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::TipcardDetail(
                resources::get_tipcard(state, &user.id, req.id).await?,
            )),
        }),
        pb::api_request::Op::GetDocument(req) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::DocumentDetail(
                resources::get_document(state, &user.id, req.id).await?,
            )),
        }),
        pb::api_request::Op::ContinueDailyReview(req) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::ContinueDailyReview(
                resources::continue_daily_review(state, &user.id, req).await?,
            )),
        }),
        pb::api_request::Op::ExploreLink(req) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::ExploredLinks(
                resources::explore_link(&req.url).await?,
            )),
        }),
        pb::api_request::Op::TestVisionModel(_) => Ok(pb::ApiResponse {
            result: Some(pb::api_response::Result::VisionModelTest(
                resources::test_vision_model(state, &user.id).await?,
            )),
        }),
        pb::api_request::Op::BootstrapApiKey(_) | pb::api_request::Op::GetApiInfo(_) => {
            unreachable!()
        }
    }
}

fn validate_review_action(action: &str) -> Result<(), (StatusCode, String)> {
    if matches!(
        action.trim(),
        "" | "again"
            | "repeat"
            | "acknowledge"
            | "acknowledged"
            | "learned"
            | "memorize"
            | "skip_known"
            | "skip_too_difficult"
            | "skip_not_interested"
            | "dismiss"
    ) {
        Ok(())
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Unsupported review action".to_string(),
        ))
    }
}

fn validate_grade(grade: u32) -> Result<u8, (StatusCode, String)> {
    u8::try_from(grade)
        .ok()
        .filter(|grade| *grade <= 5)
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "grade must be between 0 and 5".to_string(),
            )
        })
}

fn tipcard_type_value(
    value: i32,
    allow_custom: bool,
) -> Result<&'static str, (StatusCode, String)> {
    match pb::TipcardTypeValue::try_from(value).ok() {
        Some(pb::TipcardTypeValue::Repeatable) => Ok("repeatable_tip"),
        Some(pb::TipcardTypeValue::Casual) => Ok("casual_tip"),
        Some(pb::TipcardTypeValue::Manual) => Ok("manual_tip"),
        Some(pb::TipcardTypeValue::Custom) if allow_custom => Ok("custom_tip"),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "tipcard_type must be repeatable, casual, or manual".to_string(),
        )),
    }
}

fn review_action_value(value: i32) -> Result<&'static str, (StatusCode, String)> {
    match pb::ReviewActionValue::try_from(value).ok() {
        // Grade-only reviews (Again/Good/Easy on non-named-action UIs) send Unspecified.
        Some(pb::ReviewActionValue::Unspecified) => Ok(""),
        Some(pb::ReviewActionValue::Again) => Ok("again"),
        Some(pb::ReviewActionValue::Learned) => Ok("learned"),
        Some(pb::ReviewActionValue::SkipKnown) => Ok("skip_known"),
        Some(pb::ReviewActionValue::SkipNotInterested) => Ok("skip_not_interested"),
        Some(pb::ReviewActionValue::SkipTooDifficult) => Ok("skip_too_difficult"),
        Some(pb::ReviewActionValue::Acknowledge) => Ok("acknowledge"),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "action must be unspecified (grade-only), again, learned, skip_known, skip_not_interested, skip_too_difficult, or acknowledge"
                .to_string(),
        )),
    }
}

fn require_scope(
    principal: &crate::services::api_keys::ApiPrincipal,
    scope: &'static str,
) -> Result<(), (StatusCode, String)> {
    if principal.has_scope(scope) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            format!("API key requires scope '{scope}'"),
        ))
    }
}

fn required_scope(op: &pb::api_request::Op) -> &'static str {
    match op {
        pb::api_request::Op::Tips(_)
        | pb::api_request::Op::TipsV1(_)
        | pb::api_request::Op::SubmitCustomTipcard(_)
        | pb::api_request::Op::ForceDailyRefresh(_)
        | pb::api_request::Op::ContinueDailyReview(_) => "cards:write",
        pb::api_request::Op::Review(_)
        | pb::api_request::Op::ReviewV1(_)
        | pb::api_request::Op::ReviewAndAdvance(_) => "reviews:write",
        pb::api_request::Op::GetTopics(_)
        | pb::api_request::Op::ListAdminTopics(_)
        | pb::api_request::Op::ListAppTopics(_) => "topics:read",
        pb::api_request::Op::GetSummary(_)
        | pb::api_request::Op::ListTipcards(_)
        | pb::api_request::Op::ListFlowCards(_)
        | pb::api_request::Op::GetTipcard(_) => "cards:read",
        pb::api_request::Op::DeleteTipcard(_)
        | pb::api_request::Op::PinTipcard(_)
        | pb::api_request::Op::AppendTipcardImages(_)
        | pb::api_request::Op::ReplaceTipcardImages(_) => "cards:write",
        pb::api_request::Op::UpdateTopic(_) | pb::api_request::Op::DeleteTopic(_) => "topics:write",
        pb::api_request::Op::GetSettings(_) => "settings:read",
        pb::api_request::Op::UpdateSettings(_) => "settings:write",
        pb::api_request::Op::CreateApiKey(_)
        | pb::api_request::Op::CreateApiKeyV1(_)
        | pb::api_request::Op::ListApiKeys(_)
        | pb::api_request::Op::DeleteApiKey(_) => "keys:manage",
        pb::api_request::Op::ListDocuments(_) | pb::api_request::Op::GetDocument(_) => {
            "documents:read"
        }
        pb::api_request::Op::AddDocument(_)
        | pb::api_request::Op::CreateDocument(_)
        | pb::api_request::Op::UploadDocument(_)
        | pb::api_request::Op::DeleteDocument(_)
        | pb::api_request::Op::AttachDocumentTopic(_)
        | pb::api_request::Op::DetachDocumentTopic(_)
        | pb::api_request::Op::ExploreLink(_) => "documents:write",
        pb::api_request::Op::ListPoolImages(_) => "images:read",
        pb::api_request::Op::AddPoolImage(_)
        | pb::api_request::Op::CreatePoolImage(_)
        | pb::api_request::Op::DeletePoolImage(_) => "images:write",
        pb::api_request::Op::TestVisionModel(_) => "diagnostics:run",
        pb::api_request::Op::BootstrapApiKey(_) | pb::api_request::Op::GetApiInfo(_) => "*",
    }
}

fn api_op_name(op: &pb::api_request::Op) -> &'static str {
    match op {
        pb::api_request::Op::BootstrapApiKey(_) => "bootstrap_api_key",
        pb::api_request::Op::Tips(_) => "tips",
        pb::api_request::Op::SubmitCustomTipcard(_) => "submit_custom_tipcard",
        pb::api_request::Op::ForceDailyRefresh(_) => "force_daily_refresh",
        pb::api_request::Op::Review(_) => "review",
        pb::api_request::Op::GetTopics(_) => "get_topics",
        pb::api_request::Op::GetSettings(_) => "get_settings",
        pb::api_request::Op::UpdateSettings(_) => "update_settings",
        pb::api_request::Op::CreateApiKey(_) => "create_api_key",
        pb::api_request::Op::ListApiKeys(_) => "list_api_keys",
        pb::api_request::Op::DeleteApiKey(_) => "delete_api_key",
        pb::api_request::Op::ListAdminTopics(_) => "list_admin_topics",
        pb::api_request::Op::ListTipcards(_) => "list_tipcards",
        pb::api_request::Op::DeleteTipcard(_) => "delete_tipcard",
        pb::api_request::Op::PinTipcard(_) => "pin_tipcard",
        pb::api_request::Op::AppendTipcardImages(_) => "append_tipcard_images",
        pb::api_request::Op::ReplaceTipcardImages(_) => "replace_tipcard_images",
        pb::api_request::Op::DeleteTopic(_) => "delete_topic",
        pb::api_request::Op::GetSummary(_) => "get_summary",
        pb::api_request::Op::ListAppTopics(_) => "list_app_topics",
        pb::api_request::Op::UpdateTopic(_) => "update_topic",
        pb::api_request::Op::AddDocument(_) => "add_document",
        pb::api_request::Op::ListDocuments(_) => "list_documents",
        pb::api_request::Op::DeleteDocument(_) => "delete_document",
        pb::api_request::Op::AttachDocumentTopic(_) => "attach_document_topic",
        pb::api_request::Op::DetachDocumentTopic(_) => "detach_document_topic",
        pb::api_request::Op::AddPoolImage(_) => "add_pool_image",
        pb::api_request::Op::ListPoolImages(_) => "list_pool_images",
        pb::api_request::Op::DeletePoolImage(_) => "delete_pool_image",
        pb::api_request::Op::GetApiInfo(_) => "get_api_info",
        pb::api_request::Op::ListFlowCards(_) => "list_flow_cards",
        pb::api_request::Op::GetTipcard(_) => "get_tipcard",
        pb::api_request::Op::GetDocument(_) => "get_document",
        pb::api_request::Op::ContinueDailyReview(_) => "continue_daily_review",
        pb::api_request::Op::ExploreLink(_) => "explore_link",
        pb::api_request::Op::TestVisionModel(_) => "test_vision_model",
        pb::api_request::Op::CreateDocument(_) => "create_document",
        pb::api_request::Op::UploadDocument(_) => "upload_document",
        pb::api_request::Op::CreatePoolImage(_) => "create_pool_image",
        pb::api_request::Op::TipsV1(_) => "tips_v1",
        pb::api_request::Op::ReviewV1(_) => "review_v1",
        pb::api_request::Op::ReviewAndAdvance(_) => "review_and_advance",
        pb::api_request::Op::CreateApiKeyV1(_) => "create_api_key_v1",
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn operation_result_contract_accepts_expected_variant() {
        let response = pb::ApiResponse {
            result: Some(pb::api_response::Result::ApiInfo(resources::api_info())),
        };
        assert!(validate_operation_result("api_info", response).is_ok());
    }

    #[test]
    fn operation_result_contract_rejects_wrong_variant() {
        let error = validate_operation_result("api_info", empty_response()).unwrap_err();
        assert_eq!(error.0, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(error.1, "Internal server error");
    }

    #[test]
    fn review_v1_unspecified_is_grade_only_empty_action() {
        assert_eq!(
            review_action_value(pb::ReviewActionValue::Unspecified as i32).unwrap(),
            ""
        );
    }

    #[test]
    fn review_v1_acknowledge_maps_to_domain_string() {
        assert_eq!(
            review_action_value(pb::ReviewActionValue::Acknowledge as i32).unwrap(),
            "acknowledge"
        );
    }

    #[test]
    fn review_v1_skip_not_interested_is_dismiss_path() {
        assert_eq!(
            review_action_value(pb::ReviewActionValue::SkipNotInterested as i32).unwrap(),
            "skip_not_interested"
        );
    }

    #[test]
    fn governor_text_429_becomes_structured_rate_limit() {
        use axum::http::HeaderValue;

        let message = plain_error_message(StatusCode::TOO_MANY_REQUESTS, b"Too Many Requests");
        assert_eq!(message, "Too many requests; retry shortly");
        let (status, envelope) = v1_message_from_result(
            "req_rate".into(),
            Err((StatusCode::TOO_MANY_REQUESTS, message)),
        );
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        match envelope.outcome {
            Some(pb::api_v1_response::Outcome::Error(err)) => {
                assert_eq!(err.code, pb::ApiErrorCode::RateLimited as i32);
                assert!(err.retryable);
            }
            other => panic!("expected rate-limit error, got {other:?}"),
        }

        let response = v1_error_from_plain_response(
            StatusCode::TOO_MANY_REQUESTS,
            Some(HeaderValue::from_static("1")),
            None,
            b"Too Many Requests",
        );
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "1");
        assert!(is_protobuf_content_type(response.headers()));
    }

    #[test]
    fn html_error_page_is_rewritten_without_leaking_markup() {
        let message = plain_error_message(
            StatusCode::BAD_GATEWAY,
            b"<!DOCTYPE html><html>bad gateway</html>",
        );
        assert_eq!(message, "HTTP 502 response was not protobuf");

        let response = v1_error_from_plain_response(
            StatusCode::BAD_GATEWAY,
            None,
            Some("req_html"),
            b"<!DOCTYPE html><html>bad gateway</html>",
        );
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            response.headers()["x-request-id"].to_str().unwrap(),
            "req_html"
        );
    }
}
