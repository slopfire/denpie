use rand::Rng;
use serde_yaml::{Mapping, Value};
use std::path::{Path, PathBuf};

use crate::{error::AppResult, llm};

#[derive(Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub llm_model: String,
    pub llm_compress_model: String,
    pub prompt_template: String,
    pub llm_api_key: String,
    pub llm_base_url: String,
    pub llm_compress_base_url: String,
    pub llm_reasoning_effort: String,
    pub llm_compress_reasoning_effort: String,
    pub llm_compression_level: String,
    pub color_scheme: String,
    pub transparency: String,
    pub blur_intensity: String,
    pub admin_token: String,
    pub autoupdate_enabled: bool,
    pub autoupdate_repo: String,
    pub autoupdate_branch: String,
    pub autoupdate_check_interval_secs: u64,
    pub autoupdate_command: String,
    pub autoupdate_last_seen_sha: String,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub max_active_cards: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SettingsPatch {
    pub model: Option<String>,
    pub compress_model: Option<String>,
    pub template: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub compress_base_url: Option<String>,
    pub reasoning_effort: Option<String>,
    pub compress_reasoning_effort: Option<String>,
    pub compression_level: Option<String>,
    pub color_scheme: Option<String>,
    pub transparency: Option<String>,
    pub blur_intensity: Option<String>,
    pub ui_blur: Option<String>,
    pub autoupdate_enabled: Option<bool>,
    pub autoupdate_repo: Option<String>,
    pub autoupdate_branch: Option<String>,
    pub autoupdate_check_interval_secs: Option<u64>,
    pub autoupdate_command: Option<String>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub max_active_cards: Option<u64>,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> AppResult<Settings> {
        Settings::from_value(&self.load_raw()?)
    }

    pub fn load_raw(&self) -> AppResult<Value> {
        let settings_str = match std::fs::read_to_string(&self.path) {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err.into()),
        };
        let settings =
            serde_yaml::from_str(&settings_str).unwrap_or_else(|_| Value::Mapping(Mapping::new()));
        Ok(as_mapping(settings))
    }

    pub fn save_raw(&self, settings: &Value) -> AppResult<()> {
        let out = serde_yaml::to_string(settings)?;
        std::fs::write(&self.path, out)?;
        Ok(())
    }

    pub fn update(&self, patch: SettingsPatch) -> AppResult<Settings> {
        let mut raw = self.load_raw()?;
        if let Value::Mapping(ref mut map) = raw {
            put_string(map, "llm_model", patch.model);
            put_string(map, "llm_compress_model", patch.compress_model);
            put_string(map, "prompt_template", patch.template);
            put_string(map, "llm_api_key", patch.api_key);
            put_string(map, "llm_base_url", patch.base_url);
            put_string(map, "llm_compress_base_url", patch.compress_base_url);
            put_string(map, "llm_reasoning_effort", patch.reasoning_effort);
            if let Some(compression_level) = patch.compression_level {
                let level = llm::CompressionLevel::from_setting(&compression_level);
                put_string(
                    map,
                    "llm_compression_level",
                    Some(level.as_setting().to_string()),
                );
                put_string(
                    map,
                    "llm_compress_reasoning_effort",
                    Some(level.reasoning_effort().to_string()),
                );
            } else {
                put_string(
                    map,
                    "llm_compress_reasoning_effort",
                    patch.compress_reasoning_effort,
                );
            }
            put_string(map, "color_scheme", patch.color_scheme);
            put_string(map, "transparency", patch.transparency.or(patch.ui_blur));
            put_string(map, "blur_intensity", patch.blur_intensity);
            put_bool(map, "autoupdate_enabled", patch.autoupdate_enabled);
            put_string(map, "autoupdate_repo", patch.autoupdate_repo);
            put_string(map, "autoupdate_branch", patch.autoupdate_branch);
            put_u64(
                map,
                "autoupdate_check_interval_secs",
                patch.autoupdate_check_interval_secs,
            );
            put_string(
                map,
                "autoupdate_command",
                patch
                    .autoupdate_command
                    .map(|value| value.trim().to_string()),
            );
            put_string(map, "daily_time_zone", patch.daily_time_zone);
            put_string(map, "daily_update_time", patch.daily_update_time);
            put_u64(map, "max_active_cards", patch.max_active_cards);
        }
        self.save_raw(&raw)?;
        Settings::from_value(&raw)
    }

    pub fn ensure_admin_token(&self) -> AppResult<String> {
        let mut raw = self.load_raw()?;
        if let Some(token) = raw.get("admin_token").and_then(Value::as_str) {
            if !token.is_empty() {
                return Ok(token.to_string());
            }
        }

        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        if let Value::Mapping(ref mut map) = raw {
            map.insert(
                Value::String("admin_token".to_string()),
                Value::String(token.clone()),
            );
        }
        self.save_raw(&raw)?;
        Ok(token)
    }
}

impl Settings {
    pub fn from_value(settings: &Value) -> AppResult<Self> {
        let base_url = string(settings, "llm_base_url", "https://openrouter.ai/api/v1");
        let compression_level = settings
            .get("llm_compression_level")
            .and_then(Value::as_str)
            .map(llm::CompressionLevel::from_setting)
            .unwrap_or_else(|| llm::CompressionLevel::from_setting(llm::DEFAULT_COMPRESSION_LEVEL));

        Ok(Self {
            llm_model: string(settings, "llm_model", "google/gemini-3.1-flash"),
            llm_compress_model: string(
                settings,
                "llm_compress_model",
                "google/gemini-3.1-flash-lite-preview",
            ),
            prompt_template: string(settings, "prompt_template", llm::DEFAULT_PROMPT_TEMPLATE),
            llm_api_key: string(settings, "llm_api_key", ""),
            llm_base_url: base_url.clone(),
            llm_compress_base_url: string(settings, "llm_compress_base_url", &base_url),
            llm_reasoning_effort: string(settings, "llm_reasoning_effort", "none"),
            llm_compress_reasoning_effort: settings
                .get("llm_compress_reasoning_effort")
                .and_then(Value::as_str)
                .unwrap_or_else(|| compression_level.reasoning_effort())
                .to_string(),
            llm_compression_level: compression_level.as_setting().to_string(),
            color_scheme: string(settings, "color_scheme", "shadcn"),
            transparency: settings
                .get("transparency")
                .or_else(|| settings.get("ui_blur"))
                .and_then(Value::as_str)
                .unwrap_or("medium")
                .to_string(),
            blur_intensity: string(settings, "blur_intensity", "medium"),
            admin_token: string(settings, "admin_token", ""),
            autoupdate_enabled: settings
                .get("autoupdate_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            autoupdate_repo: string(settings, "autoupdate_repo", "slopfire/denpie"),
            autoupdate_branch: string(settings, "autoupdate_branch", "master"),
            autoupdate_check_interval_secs: settings
                .get("autoupdate_check_interval_secs")
                .and_then(Value::as_u64)
                .unwrap_or(3600),
            autoupdate_command: string(settings, "autoupdate_command", ""),
            autoupdate_last_seen_sha: string(settings, "autoupdate_last_seen_sha", ""),
            daily_time_zone: string(settings, "daily_time_zone", "UTC"),
            daily_update_time: string(settings, "daily_update_time", "00:00"),
            max_active_cards: settings
                .get("max_active_cards")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
    }

    pub fn apply_patch(mut self, patch: SettingsPatch) -> Self {
        if let Some(value) = patch.model {
            self.llm_model = value;
        }
        if let Some(value) = patch.compress_model {
            self.llm_compress_model = value;
        }
        if let Some(value) = patch.template {
            self.prompt_template = value;
        }
        if let Some(value) = patch.api_key {
            self.llm_api_key = value;
        }
        if let Some(value) = patch.base_url {
            self.llm_base_url = value;
        }
        if let Some(value) = patch.compress_base_url {
            self.llm_compress_base_url = value;
        }
        if let Some(value) = patch.reasoning_effort {
            self.llm_reasoning_effort = value;
        }
        if let Some(value) = patch.compression_level {
            let level = llm::CompressionLevel::from_setting(&value);
            self.llm_compression_level = level.as_setting().to_string();
            self.llm_compress_reasoning_effort = level.reasoning_effort().to_string();
        } else if let Some(value) = patch.compress_reasoning_effort {
            self.llm_compress_reasoning_effort = value;
        }
        if let Some(value) = patch.color_scheme {
            self.color_scheme = value;
        }
        if let Some(value) = patch.transparency.or(patch.ui_blur) {
            self.transparency = value;
        }
        if let Some(value) = patch.blur_intensity {
            self.blur_intensity = value;
        }
        if let Some(value) = patch.daily_time_zone {
            self.daily_time_zone = value;
        }
        if let Some(value) = patch.daily_update_time {
            self.daily_update_time = value;
        }
        if let Some(value) = patch.max_active_cards {
            self.max_active_cards = value;
        }
        self
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_value(&Value::Mapping(Mapping::new())).expect("default settings are valid")
    }
}

fn as_mapping(value: Value) -> Value {
    if value.is_mapping() {
        value
    } else {
        Value::Mapping(Mapping::new())
    }
}

fn string(settings: &Value, key: &str, default: &str) -> String {
    settings
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn put_string(map: &mut Mapping, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(Value::String(key.to_string()), Value::String(value));
    }
}

fn put_bool(map: &mut Mapping, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        map.insert(Value::String(key.to_string()), Value::Bool(value));
    }
}

fn put_u64(map: &mut Mapping, key: &str, value: Option<u64>) {
    if let Some(value) = value {
        map.insert(Value::String(key.to_string()), Value::Number(value.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_defaults_are_stable() {
        let settings = Settings::from_value(&Value::Mapping(Mapping::new())).unwrap();
        assert_eq!(settings.llm_model, "google/gemini-3.1-flash");
        assert_eq!(settings.llm_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(settings.daily_time_zone, "UTC");
        assert_eq!(settings.daily_update_time, "00:00");
        assert_eq!(settings.max_active_cards, 0);
    }

    #[test]
    fn update_normalizes_compression_level() {
        let path = std::env::temp_dir().join(format!(
            "denpie-settings-test-{}.yaml",
            rand::random::<u64>()
        ));
        let store = SettingsStore::new(path.clone());
        let settings = store
            .update(SettingsPatch {
                compression_level: Some("strong".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(settings.llm_compression_level, "strong");
        assert_eq!(
            settings.llm_compress_reasoning_effort,
            llm::CompressionLevel::from_setting("strong").reasoning_effort()
        );
        let _ = std::fs::remove_file(path);
    }
}
