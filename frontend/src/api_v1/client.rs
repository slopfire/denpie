//! Encode/decode helpers and the low-level `/api/v1` transport.

use crate::pb::{self, api_request, api_v1_response};
use gloo_net::http::Request;
use prost::Message;

const API_V1_PATH: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    pub retryable: bool,
    /// The request may have reached the server, so retrying a mutation must
    /// keep its original idempotency key.
    pub mutation_outcome_indeterminate: bool,
    pub request_id: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.request_id.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{} (request_id={})", self.message, self.request_id)
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Build a versioned envelope. Mutations **must** supply a non-empty idempotency key.
pub fn build_envelope(
    op: api_request::Op,
    request_id: impl Into<String>,
    idempotency_key: Option<String>,
    is_mutation: bool,
) -> Result<pb::ApiV1Request, ApiError> {
    if is_mutation {
        match &idempotency_key {
            Some(key) if !key.trim().is_empty() => {}
            _ => {
                return Err(ApiError {
                    status: 0,
                    message: "Idempotency key is required for mutations".to_string(),
                    retryable: false,
                    mutation_outcome_indeterminate: false,
                    request_id: String::new(),
                });
            }
        }
    }
    Ok(pb::ApiV1Request {
        request_id: request_id.into(),
        call: Some(pb::ApiRequest {
            auth: String::new(),
            op: Some(op),
        }),
        idempotency_key: idempotency_key.unwrap_or_default(),
    })
}

pub fn encode_request(envelope: &pb::ApiV1Request) -> Vec<u8> {
    envelope.encode_to_vec()
}

pub fn decode_response(bytes: &[u8]) -> Result<pb::ApiV1Response, ApiError> {
    pb::ApiV1Response::decode(bytes).map_err(|err| ApiError {
        status: 0,
        message: format!("Invalid protobuf response: {err}"),
        retryable: true,
        mutation_outcome_indeterminate: true,
        request_id: String::new(),
    })
}

fn is_protobuf_content_type(content_type: Option<&str>) -> bool {
    content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("application/x-protobuf")
                || value.eq_ignore_ascii_case("application/protobuf")
        })
}

fn error_from_non_protobuf(status: u16, bytes: &[u8]) -> ApiError {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    let message = if status == 429 {
        "Too many requests; retry shortly".to_string()
    } else if text.starts_with('<') {
        format!("HTTP {status} returned a web page instead of an API response")
    } else if !text.is_empty() && text.len() <= 300 && !text.contains('\0') {
        text.to_string()
    } else if status == 0 {
        "Invalid protobuf response".to_string()
    } else {
        format!("HTTP {status} response was not protobuf")
    };
    ApiError {
        status,
        message,
        retryable: status == 429 || (500..600).contains(&status),
        // 4xx bodies (including 429) were rejected before the handler ran.
        mutation_outcome_indeterminate: (500..600).contains(&status),
        request_id: String::new(),
    }
}

pub fn decode_http_response(
    status: u16,
    content_type: Option<&str>,
    bytes: &[u8],
) -> Result<pb::ApiV1Response, ApiError> {
    let declared_protobuf = is_protobuf_content_type(content_type);
    if content_type.is_some() && !declared_protobuf {
        return Err(error_from_non_protobuf(status, bytes));
    }
    match decode_response(bytes) {
        Ok(decoded) => Ok(decoded),
        Err(mut err) if declared_protobuf => {
            err.status = status;
            Err(err)
        }
        Err(_) => Err(error_from_non_protobuf(status, bytes)),
    }
}

/// POST a built envelope and return the success `ApiResponse` body.
pub async fn call_envelope(envelope: pb::ApiV1Request) -> ApiResult<pb::ApiResponse> {
    let body = encode_request(&envelope);
    let response = Request::post(API_V1_PATH)
        .header("Content-Type", "application/x-protobuf")
        .body(body)
        .map_err(|err| ApiError {
            status: 0,
            message: format!("Failed to build request: {err}"),
            retryable: false,
            mutation_outcome_indeterminate: false,
            request_id: envelope.request_id.clone(),
        })?
        .send()
        .await
        .map_err(|err| ApiError {
            status: 0,
            message: format!("Network error: {err}"),
            retryable: true,
            mutation_outcome_indeterminate: true,
            request_id: envelope.request_id.clone(),
        })?;

    let status = response.status();
    let content_type = response.headers().get("content-type");
    let bytes = response.binary().await.map_err(|err| ApiError {
        status,
        message: format!("Failed to read response body: {err}"),
        retryable: true,
        mutation_outcome_indeterminate: true,
        request_id: envelope.request_id.clone(),
    })?;

    let decoded = decode_http_response(status, content_type.as_deref(), &bytes)?;
    match decoded.outcome {
        Some(api_v1_response::Outcome::Success(success)) => {
            if !(200..300).contains(&status) {
                return Err(ApiError {
                    status,
                    message: format!("Unexpected HTTP {status} with success body"),
                    retryable: true,
                    mutation_outcome_indeterminate: true,
                    request_id: decoded.request_id,
                });
            }
            Ok(*success)
        }
        Some(api_v1_response::Outcome::Error(error)) => {
            let message = if error.message.is_empty() {
                format!("API error (code={})", error.code)
            } else {
                error.message
            };
            Err(ApiError {
                status,
                mutation_outcome_indeterminate: server_reports_indeterminate_outcome(
                    status,
                    &message,
                    error.retryable,
                ),
                message,
                retryable: error.retryable,
                request_id: decoded.request_id,
            })
        }
        None => Err(ApiError {
            status,
            message: "Empty API v1 response outcome".to_string(),
            retryable: true,
            mutation_outcome_indeterminate: true,
            request_id: decoded.request_id,
        }),
    }
}

/// Call a read-only operation (no idempotency key).
pub async fn call_read(op: api_request::Op) -> ApiResult<pb::ApiResponse> {
    let envelope = build_envelope(op, new_request_id("read"), None, false)?;
    call_envelope(envelope).await
}

/// Call a mutation with a generated idempotency key.
pub async fn call_mutation(op: api_request::Op) -> ApiResult<pb::ApiResponse> {
    call_mutation_with_key(op, new_idempotency_key()).await
}

/// Call a mutation with a caller-owned idempotency key. Retryable transport
/// failures are attempted once with the exact same envelope, so a response lost
/// after commit cannot execute the mutation twice.
pub async fn call_mutation_with_key(
    op: api_request::Op,
    idempotency_key: String,
) -> ApiResult<pb::ApiResponse> {
    let envelope = build_envelope(op, new_request_id("mut"), Some(idempotency_key), true)?;
    let retry_envelope = envelope.clone();
    match call_envelope(envelope).await {
        Err(err) if err.retryable && err.mutation_outcome_indeterminate => {
            call_envelope(retry_envelope).await
        }
        result => result,
    }
}

fn server_reports_indeterminate_outcome(status: u16, message: &str, retryable: bool) -> bool {
    retryable
        && (status == 409
            || message.contains("same idempotency key")
            || message.contains("idempotency result"))
}

pub fn new_request_id(prefix: &str) -> String {
    // Keep within the 1-64 ASCII charset the server accepts.
    let n = entropy_u64();
    let id = format!("{prefix}-{n}");
    id.chars().take(64).collect()
}

pub fn new_idempotency_key() -> String {
    let a = entropy_u64().to_be_bytes();
    let b = entropy_u64()
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .to_be_bytes();
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&a);
    bytes[8..].copy_from_slice(&b);
    // UUID-ish hex for the 1-128 idempotency charset.
    bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
}

fn entropy_u64() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        (js_sys::Date::now() as u64)
            .wrapping_mul(1_000_003)
            .wrapping_add((js_sys::Math::random() * 1_000_000.0) as u64)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            .wrapping_mul(0xA24B_AED4_96E9_C165)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pb::{Empty, ListFlowCardsRequest, api_request, api_response};

    #[test]
    fn mutation_without_idempotency_key_is_rejected() {
        let err = build_envelope(
            api_request::Op::DeleteTipcard(pb::DeleteByIdRequest { id: 1 }),
            "req-1",
            None,
            true,
        )
        .expect_err("mutations need a key");
        assert!(err.message.to_ascii_lowercase().contains("idempotency"));
    }

    #[test]
    fn mutation_with_empty_key_is_rejected() {
        let err = build_envelope(
            api_request::Op::PinTipcard(pb::PinTipcardRequest {
                id: 1,
                pinned: true,
            }),
            "req-2",
            Some("  ".into()),
            true,
        )
        .expect_err("blank key");
        assert!(err.message.to_ascii_lowercase().contains("idempotency"));
    }

    #[test]
    fn read_without_key_is_allowed() {
        let envelope = build_envelope(
            api_request::Op::GetApiInfo(Empty {}),
            "req-info",
            None,
            false,
        )
        .expect("reads do not need a key");
        assert!(envelope.idempotency_key.is_empty());
        assert_eq!(envelope.request_id, "req-info");
    }

    #[test]
    fn encode_decode_round_trip_preserves_op() {
        let envelope = build_envelope(
            api_request::Op::ListFlowCards(ListFlowCardsRequest {
                page_size: 12,
                page_token: String::new(),
            }),
            "round-trip",
            None,
            false,
        )
        .unwrap();
        let bytes = encode_request(&envelope);
        let decoded = pb::ApiV1Request::decode(bytes.as_slice()).expect("decode request");
        assert_eq!(decoded.request_id, "round-trip");
        match decoded.call.and_then(|c| c.op) {
            Some(api_request::Op::ListFlowCards(req)) => {
                assert_eq!(req.page_size, 12);
            }
            other => panic!("unexpected op: {other:?}"),
        }
    }

    #[test]
    fn decode_error_outcome() {
        let response = pb::ApiV1Response {
            request_id: "err-1".into(),
            outcome: Some(api_v1_response::Outcome::Error(pb::ApiError {
                code: pb::ApiErrorCode::PermissionDenied as i32,
                message: "nope".into(),
                retryable: false,
            })),
        };
        let bytes = response.encode_to_vec();
        let decoded = decode_response(&bytes).unwrap();
        match decoded.outcome {
            Some(api_v1_response::Outcome::Error(e)) => {
                assert_eq!(e.message, "nope");
                assert_eq!(decoded.request_id, "err-1");
            }
            _ => panic!("expected error outcome"),
        }
    }

    #[test]
    fn decode_success_outcome() {
        let response = pb::ApiV1Response {
            request_id: "ok-1".into(),
            outcome: Some(api_v1_response::Outcome::Success(Box::new(
                pb::ApiResponse {
                    result: Some(api_response::Result::Ok(Empty {})),
                },
            ))),
        };
        let bytes = response.encode_to_vec();
        let decoded = decode_response(&bytes).unwrap();
        match decoded.outcome {
            Some(api_v1_response::Outcome::Success(success)) => {
                assert!(matches!(success.result, Some(api_response::Result::Ok(_))));
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn idempotency_retry_classifies_definitive_and_indeterminate_failures() {
        assert!(!server_reports_indeterminate_outcome(
            500,
            "Internal server error",
            true,
        ));
        assert!(server_reports_indeterminate_outcome(
            409,
            "An identical request is still in progress",
            true,
        ));
        assert!(server_reports_indeterminate_outcome(
            500,
            "Mutation outcome could not be recorded; retry only with the same idempotency key",
            true,
        ));
    }

    #[test]
    fn text_429_is_not_decoded_as_protobuf() {
        let err = decode_http_response(429, Some("text/plain"), b"Too Many Requests")
            .expect_err("plain 429 must not be parsed as protobuf");
        assert_eq!(err.status, 429);
        assert!(err.message.to_ascii_lowercase().contains("too many"));
        assert!(err.retryable);
        assert!(
            !err.mutation_outcome_indeterminate,
            "429 is rejected before the mutation runs"
        );
    }

    #[test]
    fn html_error_page_is_not_an_end_group_tag() {
        let err = decode_http_response(
            502,
            Some("text/html"),
            b"<!DOCTYPE html><html>bad gateway</html>",
        )
        .expect_err("HTML must not be parsed as protobuf");
        assert_eq!(err.status, 502);
        assert!(err.message.contains("web page"));
        assert!(err.retryable);
        assert!(err.mutation_outcome_indeterminate);
        assert!(
            !err.message.contains("end group"),
            "got opaque protobuf error: {}",
            err.message
        );
    }

    #[test]
    fn missing_content_type_still_tries_protobuf_then_falls_back() {
        let err = decode_http_response(429, None, b"Too Many Requests")
            .expect_err("text without content-type should not become end-group");
        assert_eq!(err.status, 429);
        assert!(err.message.to_ascii_lowercase().contains("too many"));
        assert!(!err.mutation_outcome_indeterminate);
    }
}
