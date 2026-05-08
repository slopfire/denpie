use crate::AppState;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Json, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use rand::Rng;
use std::sync::Arc;
use tower_sessions::Session;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::db::repositories::{passkeys, user_settings, users};

#[derive(Clone, Debug, serde::Serialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub role: String,
    pub display_name: Option<String>,
    pub avatar_data: Option<String>,
}

#[allow(dead_code)]
pub async fn verify_api_key(
    State(state): State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let auth_header = headers.get("Authorization");

    if let Some(auth_value) = auth_header {
        let key = auth_value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;

        if state.api_keys.verify(key).await.is_ok() {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[derive(serde::Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<LoginReq>,
) -> Result<StatusCode, StatusCode> {
    let user = users::find_by_username(&state.db, req.username.trim())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let hash = user
        .password_hash
        .as_deref()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let parsed_hash = PasswordHash::new(hash).map_err(|_| StatusCode::UNAUTHORIZED)?;
    Argon2::default()
        .verify_password(req.password.as_bytes(), &parsed_hash)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    session
        .insert("user_id", user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<AuthUser>, (StatusCode, String)> {
    current_user(&state, &session).await.map(Json)
}

#[derive(serde::Deserialize)]
pub struct UpdateMeReq {
    pub display_name: Option<String>,
    pub avatar_data: Option<String>,
    pub password: Option<String>,
}

pub async fn update_me(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<UpdateMeReq>,
) -> Result<Json<AuthUser>, (StatusCode, String)> {
    let auth_user = current_user(&state, &session).await?;

    let password_hash = if let Some(pwd) = &req.password {
        if pwd.len() < 8 {
            return Err((StatusCode::BAD_REQUEST, "Password too short".to_string()));
        }
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(pwd.as_bytes(), &salt)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .to_string();
        Some(hash)
    } else {
        None
    };

    users::update(
        &state.db,
        &auth_user.id,
        req.display_name.as_deref(),
        req.avatar_data.as_deref(),
        password_hash.as_deref(),
    )
    .await
    .map_err(|err| err.into_status_body())?;

    current_user(&state, &session).await.map(Json)
}

pub async fn delete_me(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<StatusCode, (StatusCode, String)> {
    let auth_user = current_user(&state, &session).await?;
    users::delete(&state.db, &auth_user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    session.delete().await.ok();
    Ok(StatusCode::OK)
}

pub async fn logout(session: Session) -> Result<StatusCode, StatusCode> {
    session
        .delete()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

#[derive(serde::Deserialize)]
pub struct SetupReq {
    pub admin_token: String,
    pub username: String,
    pub password: String,
}

pub async fn setup(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<SetupReq>,
) -> Result<Json<AuthUser>, (StatusCode, String)> {
    let settings = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    if settings.admin_token.is_empty() || req.admin_token != settings.admin_token {
        return Err((StatusCode::UNAUTHORIZED, "Invalid setup token".to_string()));
    }
    if !users::setup_allowed(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
    {
        return Err((StatusCode::CONFLICT, "Setup already complete".to_string()));
    }
    let username = req.username.trim();
    if username.is_empty() || req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Username and password of at least 8 characters are required".to_string(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .to_string();
    let user_id = new_user_id();
    let role = if users::count(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
        == 0
    {
        "admin"
    } else {
        "user"
    };

    let user = users::create(&state.db, &user_id, username, &password_hash, role)
        .await
        .map_err(|err| err.into_status_body())?;
    users::claim_unowned_rows(&state.db, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;
    user_settings::upsert(&state.db, &user.id, &settings)
        .await
        .map_err(|err| err.into_status_body())?;
    session
        .insert("user_id", &user.id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(AuthUser {
        id: user.id,
        username: user.username,
        role: user.role,
        display_name: user.display_name,
        avatar_data: user.avatar_data,
    }))
}

pub async fn require_session(
    session: Session,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user: Option<String> = session.get("user_id").await.unwrap_or(None);
    if user.is_some() {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn current_user(
    state: &AppState,
    session: &Session,
) -> Result<AuthUser, (StatusCode, String)> {
    let user_id: Option<String> = session
        .get("user_id")
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let user_id =
        user_id.ok_or_else(|| (StatusCode::UNAUTHORIZED, "Login required".to_string()))?;
    let user = users::find_by_id(&state.db, &user_id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(AuthUser {
        id: user.id,
        username: user.username,
        role: user.role,
        display_name: user.display_name,
        avatar_data: user.avatar_data,
    })
}

fn new_user_id() -> String {
    let raw: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    format!("usr_{raw}")
}

pub async fn register_start(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<CreationChallengeResponse>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let user_unique_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, user.id.as_bytes());

    let (ccr, reg_state) = state
        .webauthn
        .start_passkey_registration(
            user_unique_id,
            &user.username,
            user.display_name.as_deref().unwrap_or(&user.username),
            None,
        )
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    session
        .insert("reg_state", reg_state)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(ccr))
}

pub async fn register_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let reg_state: PasskeyRegistration = session
        .get("reg_state")
        .await
        .unwrap_or(None)
        .ok_or((StatusCode::BAD_REQUEST, "Missing registration state".to_string()))?;
    session.remove::<PasskeyRegistration>("reg_state").await.ok();

    let passkey = state
        .webauthn
        .finish_passkey_registration(&reg, &reg_state)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    passkeys::save(&state.db, &user.id, &passkey)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(StatusCode::OK)
}

pub async fn login_passkey_start(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<RequestChallengeResponse>, (StatusCode, String)> {
    let (rcr, auth_state) = state
        .webauthn
        .start_passkey_authentication(&[])
        .map_err(|err: WebauthnError| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    session
        .insert("auth_state", auth_state)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(rcr))
}

pub async fn login_passkey_finish(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(auth): Json<PublicKeyCredential>,
) -> Result<StatusCode, (StatusCode, String)> {
    let auth_state: PasskeyAuthentication = session
        .get("auth_state")
        .await
        .unwrap_or(None)
        .ok_or((StatusCode::BAD_REQUEST, "Missing authentication state".to_string()))?;
    session.remove::<PasskeyAuthentication>("auth_state").await.ok();

    let _auth_result = state
        .webauthn
        .finish_passkey_authentication(&auth, &auth_state)
        .map_err(|err: WebauthnError| (StatusCode::UNAUTHORIZED, err.to_string()))?;

    // Find user by credential ID instead of UUID for now, as it's easier to map
    let cred_id = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &auth.id,
    )
    .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid ID".to_string()))?;

    let (user_id, _) = passkeys::find_by_id(&state.db, &cred_id)
        .await
        .map_err(|err| err.into_status_body())?
        .ok_or((StatusCode::UNAUTHORIZED, "Passkey not found".to_string()))?;

    session
        .insert("user_id", user_id)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(StatusCode::OK)
}

pub async fn list_passkeys(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let passkeys = passkeys::list(&state.db, &user.id)
        .await
        .map_err(|err| err.into_status_body())?;

    let result = passkeys
        .into_iter()
        .map(|pk| {
            serde_json::json!({
                "id": base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, pk.cred_id()),
                "name": pk.cred_id().iter().map(|b| format!("{:02x}", b)).collect::<String>().chars().take(8).collect::<String>(), // Placeholder name
            })
        })
        .collect();

    Ok(Json(result))
}

pub async fn delete_passkey(
    State(state): State<Arc<AppState>>,
    session: Session,
    axum::extract::Path(id_base64): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user = current_user(&state, &session).await?;
    let id = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, id_base64)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid ID".to_string()))?;

    passkeys::delete(&state.db, &user.id, &id)
        .await
        .map_err(|err| err.into_status_body())?;

    Ok(StatusCode::OK)
}
