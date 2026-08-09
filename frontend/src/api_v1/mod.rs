//! Browser client for Denpie API v1 (`POST /api/v1` protobuf).
//!
//! Session cookies authorize same-origin calls after normal login (server-side
//! session bridge). Mutations always send an idempotency key.

mod client;
mod ops;

#[allow(unused_imports)] // re-exports for call sites and unit tests
pub use client::{
    ApiError, ApiResult, build_envelope, decode_response, encode_request, new_idempotency_key,
    new_request_id,
};
pub use ops::*;
