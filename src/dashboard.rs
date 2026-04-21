use axum::{extract::State, response::Html, Json};
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
    .premium-btn { background: var(--color-accent); color: #000; font-weight: 600; transition: transform 0.2s, opacity 0.2s; }
    .premium-btn:hover { opacity: 0.9; transform: translateY(-1px); }
  </style>
</head>
<body class="min-h-screen flex flex-col items-center py-12 px-4">
  <header class="w-full max-w-5xl mb-12 flex justify-between items-center">
    <h1 class="text-4xl font-extrabold tracking-tighter">DAILY<span class="text-text-dim">TIP</span></h1>
    <div class="text-sm tracking-widest uppercase text-text-dim">Admin Portal</div>
  </header>

  <main class="w-full max-w-5xl grid grid-cols-1 lg:grid-cols-2 gap-8">
    <section class="glass p-8 rounded-3xl relative overflow-hidden group">
      <div class="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
      <h2 class="text-2xl font-semibold mb-6 tracking-tight relative z-10">Configuration</h2>
      <form id="settingsForm" class="space-y-6 relative z-10">
        <div>
          <label class="block text-xs uppercase tracking-widest text-text-dim mb-2">LLM Model</label>
          <input type="text" class="premium-input w-full rounded-xl p-4" value="google/gemini-3.1-flash" />
        </div>
        <div>
          <label class="block text-xs uppercase tracking-widest text-text-dim mb-2">Prompt Template</label>
          <textarea class="premium-input w-full rounded-xl p-4 h-40 resize-none"></textarea>
        </div>
        <button class="premium-btn w-full py-4 rounded-xl shadow-lg">Save Configuration</button>
      </form>
    </section>

    <section class="glass p-8 rounded-3xl relative overflow-hidden group">
      <div class="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
      <h2 class="text-2xl font-semibold mb-6 tracking-tight relative z-10">Access Keys</h2>
      <div class="space-y-6 relative z-10">
        <button class="premium-input w-full py-4 rounded-xl border-dashed hover:border-accent transition-colors flex items-center justify-center gap-2">
          <span>+</span> Generate New Key
        </button>
        <div class="space-y-3">
          <div class="premium-input rounded-xl p-4 flex justify-between items-center">
            <div class="font-mono text-sm text-text-dim">sk_live_...</div>
            <div class="text-xs uppercase tracking-widest bg-white/10 px-2 py-1 rounded">Active</div>
          </div>
        </div>
      </div>
    </section>
  </main>
</body>
</html>
    "#)
}

#[derive(Serialize)]
pub struct SettingsRes {
    model: String,
    template: String,
}

pub async fn get_settings(_state: State<Arc<AppState>>) -> Json<SettingsRes> {
    let settings_str = fs::read_to_string("settings.yaml").unwrap_or_default();
    let settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or_default();
    
    let model = settings.get("llm_model").and_then(|v| v.as_str()).unwrap_or("google/gemini-3.1-flash").to_string();
    let template = settings.get("prompt_template").and_then(|v| v.as_str()).unwrap_or("Give a smart tip about {topic}.").to_string();

    Json(SettingsRes { model, template })
}

#[derive(Deserialize)]
pub struct UpdateSettingsReq {
    model: String,
    template: String,
}

pub async fn update_settings(_state: State<Arc<AppState>>, Json(req): Json<UpdateSettingsReq>) -> Json<()> {
    let settings_str = fs::read_to_string("settings.yaml").unwrap_or_default();
    let mut settings: serde_yaml::Value = serde_yaml::from_str(&settings_str).unwrap_or(serde_yaml::Value::Mapping(Default::default()));
    
    if let serde_yaml::Value::Mapping(ref mut map) = settings {
        map.insert(serde_yaml::Value::String("llm_model".to_string()), serde_yaml::Value::String(req.model));
        map.insert(serde_yaml::Value::String("prompt_template".to_string()), serde_yaml::Value::String(req.template));
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
