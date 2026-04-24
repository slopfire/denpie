use axum::{extract::State, response::Html, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::AppState;
use rand::Rng;
use sha2::{Sha256, Digest};
use std::fs;

pub async fn index() -> Html<&'static str> {
    Html(r#"
<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <script src="https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4"></script>
  <link href="https://fonts.googleapis.com/css2?family=Outfit:wght@300;400;600;800&display=swap" rel="stylesheet">
  <style type="text/tailwindcss">
    @theme {
      --font-sans: 'Outfit', sans-serif;
      --color-bg-dark: #0a0a0a;
      --color-surface: #111111;
      --color-border: #222222;
      --color-accent: #f8f8f8;
      --color-text-dim: #888888;
    }
    body { background-color: var(--color-bg-dark); color: #fff; font-family: var(--font-sans); }
    .glass { background: rgba(17, 17, 17, 0.7); backdrop-filter: blur(16px); border: 1px solid var(--color-border); }
    .premium-input { background: #000; border: 1px solid var(--color-border); color: #fff; transition: border 0.3s ease; }
    .premium-input:focus { border-color: var(--color-accent); outline: none; }
    .premium-btn { background: var(--color-accent); color: #000; font-weight: 600; transition: transform 0.2s, opacity 0.2s; cursor: pointer; }
    .premium-btn:hover { opacity: 0.9; transform: translateY(-1px); }
  </style>
</head>
<body class="min-h-screen flex flex-col items-center py-12 px-4">
  <header class="w-full max-w-5xl mb-12 flex justify-between items-center">
    <h1 class="text-4xl font-extrabold tracking-tighter">DAILY<span class="text-text-dim">TIP</span></h1>
    <div class="text-sm tracking-widest uppercase text-text-dim" id="auth-status">Admin Portal</div>
  </header>

  <main id="auth-panel" class="w-full max-w-md glass p-8 rounded-3xl mb-8 flex flex-col gap-4 text-center hidden">
      <h2 class="text-2xl font-semibold mb-2">Authentication Required</h2>
      <input type="password" id="adminTokenInput" class="premium-input w-full rounded-xl p-4" placeholder="Enter Admin Token" />
      <button id="btnLogin" class="premium-btn py-3 rounded-xl shadow-lg">Login</button>
      <div id="authError" class="text-red-500 text-sm mt-2"></div>
  </main>

  <main id="dashboard-panel" class="w-full max-w-5xl grid grid-cols-1 lg:grid-cols-2 gap-8 hidden">
    <section class="glass p-8 rounded-3xl relative overflow-hidden group">
      <div class="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
      <h2 class="text-2xl font-semibold mb-6 tracking-tight relative z-10">Configuration</h2>
      <form id="settingsForm" class="space-y-6 relative z-10">
        <div>
          <label class="block text-xs uppercase tracking-widest text-text-dim mb-2">LLM Model</label>
          <input type="text" id="modelInput" class="premium-input w-full rounded-xl p-4" value="google/gemini-3.1-flash" />
        </div>
        <div>
          <label class="block text-xs uppercase tracking-widest text-text-dim mb-2">Prompt Template</label>
          <textarea id="templateInput" class="premium-input w-full rounded-xl p-4 h-40 resize-none"></textarea>
        </div>
        <div>
          <label class="block text-xs uppercase tracking-widest text-text-dim mb-2">LLM API Key</label>
          <input type="password" id="apiKeyInput" class="premium-input w-full rounded-xl p-4" placeholder="sk-..." />
        </div>
        <div>
          <label class="block text-xs uppercase tracking-widest text-text-dim mb-2">LLM Base URL</label>
          <input type="text" id="baseUrlInput" class="premium-input w-full rounded-xl p-4" value="https://openrouter.ai/api/v1" />
        </div>
        <button type="submit" class="premium-btn w-full py-4 rounded-xl shadow-lg">Save Configuration</button>
      </form>
    </section>

    <section class="glass p-8 rounded-3xl relative overflow-hidden group">
      <div class="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
      <h2 class="text-2xl font-semibold mb-6 tracking-tight relative z-10">Access Keys</h2>
      <div class="space-y-6 relative z-10">
        <button id="generateKeyBtn" class="premium-input w-full py-4 rounded-xl border-dashed hover:border-accent transition-colors flex items-center justify-center gap-2">
          <span>+</span> Generate New Key
        </button>
        <div id="keysList" class="space-y-3">
          <div class="premium-input rounded-xl p-4 flex justify-between items-center">
            <div class="font-mono text-sm text-text-dim">sk_live_...</div>
            <div class="text-xs uppercase tracking-widest bg-white/10 px-2 py-1 rounded">Active</div>
          </div>
        </div>
      </div>
    </section>
  </main>
    <script>
      const authPanel = document.getElementById('auth-panel');
      const dashboardPanel = document.getElementById('dashboard-panel');
      const authStatus = document.getElementById('auth-status');
      const authError = document.getElementById('authError');
      const adminTokenInput = document.getElementById('adminTokenInput');

      // Check if logged in and load settings
      async function loadKeys() {
        const res = await fetch('/admin/keys');
        if (res.ok) {
          const keys = await res.json();
          const list = document.getElementById('keysList');
          list.innerHTML = '';
          for (const key of keys) {
            list.innerHTML += `<div class="premium-input rounded-xl p-4 flex justify-between items-center">
              <div>
                <div class="font-mono text-sm text-accent">${key.client_name}</div>
                <div class="text-xs text-text-dim opacity-50">${key.created_at}</div>
              </div>
              <div class="flex items-center gap-2">
                <div class="text-xs uppercase tracking-widest bg-white/10 px-2 py-1 rounded">Active</div>
                <button onclick="deleteKey(${key.id})" class="text-xs uppercase tracking-widest bg-red-500/20 text-red-500 px-2 py-1 rounded hover:bg-red-500/40 cursor-pointer">Delete</button>
              </div>
            </div>`;
          }
        }
      }

      async function deleteKey(id) {
        if (!confirm('Are you sure you want to delete this key?')) return;
        const res = await fetch('/admin/keys', {
          method: 'DELETE',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ id })
        });
        if (res.ok) {
          await loadKeys();
        } else {
          alert('Failed to delete key.');
        }
      }

      async function loadSettings() {
        const res = await fetch('/admin/settings');
        if (res.ok) {
          const data = await res.json();
          document.getElementById('modelInput').value = data.model || 'google/gemini-3.1-flash';
          document.getElementById('templateInput').value = data.template || 'Give a smart tip about {topic}.';
          document.getElementById('apiKeyInput').value = data.api_key || '';
          document.getElementById('baseUrlInput').value = data.base_url || 'https://openrouter.ai/api/v1';
          dashboardPanel.classList.remove('hidden');
          await loadKeys();
          return true;
        }
        return false;
      }

      loadSettings().then(success => {
        if (!success) authPanel.classList.remove('hidden');
      });

      document.getElementById('settingsForm').addEventListener('submit', async (e) => {
        e.preventDefault();
        const model = document.getElementById('modelInput').value;
        const template = document.getElementById('templateInput').value;
        const api_key = document.getElementById('apiKeyInput').value;
        const base_url = document.getElementById('baseUrlInput').value;
        const res = await fetch('/admin/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ model, template, api_key, base_url })
        });
        if (res.ok) {
          alert('Settings saved successfully!');
        } else {
          alert('Failed to save settings.');
        }
      });

      document.getElementById('generateKeyBtn').addEventListener('click', async () => {
        const client_name = prompt('Enter a name for this key:', 'dashboard_gen');
        if (!client_name) return;
        const res = await fetch('/admin/keys', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ client_name })
        });
        if (res.ok) {
          const key = await res.json();
          alert('New Key generated: ' + key + '\n\nPlease save it now, it will not be shown again.');
          await loadKeys();
        } else {
          alert('Failed to generate key.');
        }
      });

      document.getElementById('btnLogin').addEventListener('click', async () => {
        authError.textContent = '';
        const token = adminTokenInput.value.trim();
        if (!token) return;

        try {
          const resp = await fetch('/auth/login', { 
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ admin_token: token })
          });
          
          if (resp.ok) {
            authPanel.classList.add('hidden');
            await loadSettings();
          } else {
            throw new Error('Login failed. Invalid token.');
          }
        } catch (err) {
          authError.style.color = 'red';
          authError.textContent = err.message;
        }
      });
    </script>
  </body>
</html>
    "#)
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
