use std::sync::Arc;

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use tower_sessions::Session;

use crate::AppState;
use crate::auth::new_user_id;
use crate::dashboard::response::{CreateUserReq, UpdateUserReq, UserInfo};
use crate::dashboard::util::require_admin;
use crate::db::repositories::{user_settings, users};

pub async fn list_users(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Result<Json<Vec<UserInfo>>, (StatusCode, String)> {
    let _admin = require_admin(&state, &session).await?;
    let entries = users::list_all(&state.db)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(Json(
        entries
            .into_iter()
            .map(|e| UserInfo {
                id: e.id,
                username: e.username,
                role: e.role,
                display_name: e.display_name,
                created_at: e.created_at,
            })
            .collect(),
    ))
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<CreateUserReq>,
) -> Result<Json<UserInfo>, (StatusCode, String)> {
    let admin = require_admin(&state, &session).await?;

    let username = req.username.trim();
    if username.is_empty() || req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Username and password of at least 8 characters are required".to_string(),
        ));
    }
    if req.role != "user" && req.role != "admin" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Role must be 'user' or 'admin'".to_string(),
        ));
    }

    if users::find_by_username(&state.db, username)
        .await
        .map_err(|err| err.into_status_body())?
        .is_some()
    {
        return Err((StatusCode::CONFLICT, "Username already taken".to_string()));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .to_string();

    let user_id = new_user_id();
    let user = users::create(&state.db, &user_id, username, &password_hash, &req.role)
        .await
        .map_err(|err| err.into_status_body())?;
    if let Some(name) = &req.display_name {
        let name = name.trim();
        if !name.is_empty() {
            users::update(&state.db, &user.id, Some(name), None, None)
                .await
                .map_err(|err| err.into_status_body())?;
        }
    }

    // Initialize per-user settings with server defaults so the new user has a
    // working configuration without needing to visit the settings page first.
    let settings = state
        .settings
        .get_settings()
        .map_err(|err| err.into_status_body())?;
    user_settings::upsert(&state.db, &user.id, &settings)
        .await
        .map_err(|err| err.into_status_body())?;

    let entry = users::list_all(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
        .into_iter()
        .find(|e| e.id == user.id)
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Created user not found".to_string(),
            )
        })?;

    let _ = admin; // admin verified, not used further
    Ok(Json(UserInfo {
        id: entry.id,
        username: entry.username,
        role: entry.role,
        display_name: entry.display_name,
        created_at: entry.created_at,
    }))
}

pub async fn update_user(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserReq>,
) -> Result<Json<UserInfo>, (StatusCode, String)> {
    let admin = require_admin(&state, &session).await?;

    if let Some(role) = &req.role {
        if role != "user" && role != "admin" {
            return Err((
                StatusCode::BAD_REQUEST,
                "Role must be 'user' or 'admin'".to_string(),
            ));
        }
        // Block self-demotion that would lock out the last admin.
        if role != "admin" && admin.id == id {
            let count = users::admin_count(&state.db)
                .await
                .map_err(|err| err.into_status_body())?;
            if count <= 1 {
                return Err((
                    StatusCode::CONFLICT,
                    "Cannot demote the last admin".to_string(),
                ));
            }
        }
        users::update_role(&state.db, &id, role)
            .await
            .map_err(|err| err.into_status_body())?;
    }

    if let Some(password) = &req.password {
        if password.len() < 8 {
            return Err((StatusCode::BAD_REQUEST, "Password too short".to_string()));
        }
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .to_string();
        users::update(&state.db, &id, None, None, Some(&hash))
            .await
            .map_err(|err| err.into_status_body())?;
    }

    let entry = users::list_all(&state.db)
        .await
        .map_err(|err| err.into_status_body())?
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| (StatusCode::NOT_FOUND, "User not found".to_string()))?;

    Ok(Json(UserInfo {
        id: entry.id,
        username: entry.username,
        role: entry.role,
        display_name: entry.display_name,
        created_at: entry.created_at,
    }))
}

pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let admin = require_admin(&state, &session).await?;

    // Prevent self-deletion.
    if admin.id == id {
        return Err((
            StatusCode::CONFLICT,
            "Cannot delete your own account".to_string(),
        ));
    }

    // Prevent deleting the last admin.
    let target = users::find_by_id(&state.db, &id)
        .await
        .map_err(|err| err.into_status_body())?;
    if target.role == "admin" {
        let count = users::admin_count(&state.db)
            .await
            .map_err(|err| err.into_status_body())?;
        if count <= 1 {
            return Err((
                StatusCode::CONFLICT,
                "Cannot delete the last admin".to_string(),
            ));
        }
    }

    users::delete(&state.db, &id)
        .await
        .map_err(|err| err.into_status_body())?;
    Ok(StatusCode::OK)
}
