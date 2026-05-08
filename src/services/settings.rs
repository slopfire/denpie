use crate::{
    config::{Settings, SettingsPatch, SettingsStore},
    error::AppResult,
};

#[derive(Clone)]
pub struct SettingsService {
    store: SettingsStore,
}

impl SettingsService {
    pub fn new(store: SettingsStore) -> Self {
        Self { store }
    }

    pub fn get_settings(&self) -> AppResult<Settings> {
        self.store.load()
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> AppResult<Settings> {
        self.store.update(patch)
    }

    pub fn ensure_admin_token(&self) -> AppResult<String> {
        self.store.ensure_admin_token()
    }

    pub fn store(&self) -> &SettingsStore {
        &self.store
    }
}
