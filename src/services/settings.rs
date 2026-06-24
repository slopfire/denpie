use crate::{
    AppState,
    config::{Settings, SettingsPatch, SettingsStore},
    db::repositories::user_settings,
    error::AppResult,
};

pub struct SettingsService {
    store: SettingsStore,
    cache: std::sync::RwLock<Option<Settings>>,
}

impl SettingsService {
    pub fn new(store: SettingsStore) -> Self {
        Self {
            store,
            cache: std::sync::RwLock::new(None),
        }
    }

    pub fn get_settings(&self) -> AppResult<Settings> {
        {
            let guard = self.cache.read().unwrap_or_else(|e| e.into_inner());
            if let Some(settings) = guard.as_ref() {
                return Ok(settings.clone());
            }
        }
        let settings = self.store.load()?;
        {
            let mut guard = self.cache.write().unwrap_or_else(|e| e.into_inner());
            *guard = Some(settings.clone());
        }
        Ok(settings)
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> AppResult<Settings> {
        let settings = self.store.update(patch)?;
        {
            let mut guard = self.cache.write().unwrap_or_else(|e| e.into_inner());
            *guard = None;
        }
        Ok(settings)
    }

    pub fn ensure_admin_token(&self) -> AppResult<String> {
        let token = self.store.ensure_admin_token()?;
        {
            let mut guard = self.cache.write().unwrap_or_else(|e| e.into_inner());
            *guard = None;
        }
        Ok(token)
    }

    pub fn store(&self) -> &SettingsStore {
        &self.store
    }

    pub async fn user_settings_get(state: &AppState, user_id: &str) -> AppResult<Settings> {
        let defaults = state.settings.get_settings()?;
        user_settings::get(&state.db, user_id, defaults).await
    }

    pub async fn user_settings_upsert(
        state: &AppState,
        user_id: &str,
        settings: &Settings,
    ) -> AppResult<()> {
        user_settings::upsert(&state.db, user_id, settings).await
    }
}
