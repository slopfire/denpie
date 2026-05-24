use crate::{
    config::{Settings, SettingsPatch, SettingsStore},
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
}
