use axum::{extract::State, response::{Html, IntoResponse, Response}, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::AppState;
use rand::Rng;
use sha2::{Sha256, Digest};
use std::fs;

pub async fn index() -> Response {
    match fs::read_to_string("templates/admin.html") {
        Ok(html) => Html(html).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Admin template missing").into_response(),
    }
}

#[derive(Serialize)]
pub struct SettingsRes {
    model: String,
    template: String,
    api_key: String,
    base_url: String,
}

pub async fn get_settings(_state: State<Arc<AppState>>) -> Json<SettingsRes> {
    let settings_str = fs::read_to_string("settings.yaml").unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or_default();
    
    let model = settings.get("llm_model").and_then(|v| v.as_str()).unwrap_or("google/gemini-3.1-flash").to_string();
    let template = settings.get("prompt_template").and_then(|v| v.as_str()).unwrap_or("Give a smart tip about {topic}.").to_string();
    let api_key = settings.get("llm_api_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let base_url = settings.get("llm_base_url").and_then(|v| v.as_str()).unwrap_or("https://openrouter.ai/api/v1").to_string();

    Json(SettingsRes { model, template, api_key, base_url })
}

#[derive(Deserialize)]
pub struct UpdateSettingsReq {
    model: String,
    template: String,
    api_key: String,
    base_url: String,
}

pub async fn update_settings(_state: State<Arc<AppState>>, Json(req): Json<UpdateSettingsReq>) -> Json<()> {
    let settings_str = fs::read_to_string("settings.yaml").unwrap_or_default();
    let mut settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or(serde_yaml::Value::Mapping(Default::default()));
    if !settings.is_mapping() {
        settings = serde_yaml::Value::Mapping(Default::default());
    }
    
    if let serde_yaml::Value::Mapping(ref mut map) = settings {
        map.insert(serde_yaml::Value::String("llm_model".to_string()), serde_yaml::Value::String(req.model));
        map.insert(serde_yaml::Value::String("prompt_template".to_string()), serde_yaml::Value::String(req.template));
        map.insert(serde_yaml::Value::String("llm_api_key".to_string()), serde_yaml::Value::String(req.api_key));
        map.insert(serde_yaml::Value::String("llm_base_url".to_string()), serde_yaml::Value::String(req.base_url));
    }

    let out_str = serde_yaml::to_string(&settings).unwrap();
    fs::write("settings.yaml", out_str).unwrap();

    Json(())
}

#[derive(Deserialize)]
pub struct CreateKeyReq {
    pub client_name: Option<String>,
}

pub async fn create_api_key(State(state): State<Arc<AppState>>, req: Option<Json<CreateKeyReq>>) -> Json<String> {
    let raw_key: String = rand::thread_rng().sample_iter(&rand::distributions::Alphanumeric).take(32).map(char::from).collect();
    let api_key = format!("sk_live_{}", raw_key);
    
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    let client_name = req.and_then(|Json(r)| r.client_name).unwrap_or_else(|| "default_client".to_string());

    let _ = sqlx::query!("INSERT INTO api_keys (key_hash, client_name) VALUES (?, ?)", key_hash, client_name)
        .execute(&state.db).await;

    Json(api_key)
}

#[derive(Serialize, Deserialize)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

pub async fn list_api_keys(State(state): State<Arc<AppState>>) -> Json<Vec<ApiKeyInfo>> {
    let rows = sqlx::query!("SELECT id, client_name, created_at FROM api_keys ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    
    let keys = rows.into_iter().map(|row| ApiKeyInfo {
        id: row.id,
        client_name: row.client_name,
        created_at: row.created_at.map(|d| d.to_string()).unwrap_or_default(),
    }).collect();
    
    Json(keys)
}

#[derive(Deserialize)]
pub struct DeleteKeyReq {
    pub id: i64,
}

pub async fn delete_api_key(State(state): State<Arc<AppState>>, Json(req): Json<DeleteKeyReq>) -> StatusCode {
    let result = sqlx::query!("DELETE FROM api_keys WHERE id = ?", req.id)
        .execute(&state.db)
        .await;
    
    if result.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
