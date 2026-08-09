use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use crate::{AppState, db::repositories};

use super::auth::{request_api_key, require_api_key};

pub async fn tipcard_image(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, (StatusCode, String)> {
    let user = authenticate(&state, &headers, "cards:read").await?;
    let image = repositories::tipcards::find_image(&state.db, &user.id, id)
        .await
        .map_err(|err| err.into_status_body())?;
    serve_image(
        &state,
        &headers,
        &image.storage_path,
        &image.mime_type,
        image.byte_size,
        &format!("tipcard-image-{}-{}", image.id, image.byte_size),
    )
    .await
}

pub async fn pool_image(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, (StatusCode, String)> {
    let user = authenticate(&state, &headers, "images:read").await?;
    let image = repositories::image_pool::find_pool_image(&state.db, &user.id, id)
        .await
        .map_err(|err| err.into_status_body())?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Pool image not found".to_string()))?;
    serve_image(
        &state,
        &headers,
        &image.storage_path,
        &image.mime_type,
        image.byte_size,
        &format!("pool-image-{}-{}", image.id, image.byte_size),
    )
    .await
}

async fn authenticate(
    state: &AppState,
    headers: &HeaderMap,
    scope: &'static str,
) -> Result<crate::auth::AuthUser, (StatusCode, String)> {
    let api_key = request_api_key(headers, "")?;
    let principal = require_api_key(state, &api_key).await?;
    if !principal.has_scope(scope) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("API key requires scope '{scope}'"),
        ));
    }
    Ok(principal.user)
}

async fn serve_image(
    state: &AppState,
    headers: &HeaderMap,
    storage_path: &str,
    mime_type: &str,
    byte_size: i64,
    etag_value: &str,
) -> Result<Response, (StatusCode, String)> {
    let etag = format!("\"{etag_value}\"");
    let etag_header = HeaderValue::from_str(&etag).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid image metadata".to_string(),
        )
    })?;
    if etag_matches(headers.get(header::IF_NONE_MATCH), &etag) {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("private, no-cache, max-age=0, must-revalidate"),
                ),
                (header::ETAG, etag_header),
            ],
        )
            .into_response());
    }

    let bytes = tokio::fs::read(state.image_dir.join(storage_path))
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Image file not found".to_string()))?;
    if byte_size >= 0 && bytes.len() as i64 != byte_size {
        tracing::warn!(
            storage_path,
            expected_bytes = byte_size,
            actual_bytes = bytes.len(),
            "stored image size differs from database metadata"
        );
    }
    let content_type = HeaderValue::from_str(mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-cache, max-age=0, must-revalidate"),
            ),
            (header::ETAG, etag_header),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
        ],
        bytes,
    )
        .into_response())
}

fn etag_matches(if_none_match: Option<&HeaderValue>, etag: &str) -> bool {
    if_none_match
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == "*" || candidate == etag)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::etag_matches;
    use axum::http::HeaderValue;

    #[test]
    fn matches_etag_lists() {
        assert!(etag_matches(
            Some(&HeaderValue::from_static("\"other\", \"wanted\"")),
            "\"wanted\""
        ));
        assert!(!etag_matches(
            Some(&HeaderValue::from_static("\"other\"")),
            "\"wanted\""
        ));
    }
}
