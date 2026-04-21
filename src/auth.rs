use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
    http::StatusCode,
};
use webauthn_rs::Webauthn;
use std::sync::Arc;
use crate::AppState;

pub async fn verify_api_key(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let auth_header = headers.get("Authorization");

    if let Some(auth_value) = auth_header {
        let key = auth_value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        
        let mut hasher = sha2::Sha256::new();
        sha2::Digest::update(&mut hasher, key.as_bytes());
        let key_hash = hex::encode(hasher.finalize());

        let exists: Option<String> = sqlx::query_scalar!(
            "SELECT client_name FROM api_keys WHERE key_hash = ?",
            key_hash
        )
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        if exists.is_some() {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub fn init_webauthn() -> Webauthn {
    let rp_id = "localhost";
    let rp_origin = url::Url::parse("http://localhost:3000").unwrap();
    let builder = webauthn_rs::WebauthnBuilder::new(rp_id, &rp_origin).unwrap();
    builder.build().unwrap()
}
