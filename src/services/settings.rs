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

    /// Apply one settings patch consistently for browser sessions and API keys.
    /// Instance-wide appearance/autoupdate defaults are admin-only; per-user LLM,
    /// grounding, and scheduling values are stored in PostgreSQL.
    pub async fn update_user_settings(
        state: &AppState,
        user_id: &str,
        is_admin: bool,
        patch: SettingsPatch,
    ) -> AppResult<()> {
        let requeue_image_jobs = patch.image_strategy.is_some();
        let updates_instance_defaults = patch.color_scheme.is_some()
            || patch.transparency.is_some()
            || patch.blur_intensity.is_some()
            || patch.autoupdate_enabled.is_some()
            || patch.autoupdate_repo.is_some()
            || patch.autoupdate_branch.is_some()
            || patch.autoupdate_check_interval_secs.is_some()
            || patch.autoupdate_command.is_some();
        if updates_instance_defaults && !is_admin {
            return Err(crate::error::AppError::Forbidden(
                "Instance appearance and autoupdate settings require an admin API key".to_string(),
            ));
        }

        if updates_instance_defaults {
            state.settings.update_settings(SettingsPatch {
                color_scheme: patch.color_scheme.clone(),
                transparency: patch.transparency.clone(),
                blur_intensity: patch.blur_intensity.clone(),
                autoupdate_enabled: patch.autoupdate_enabled,
                autoupdate_repo: patch.autoupdate_repo.clone(),
                autoupdate_branch: patch.autoupdate_branch.clone(),
                autoupdate_check_interval_secs: patch.autoupdate_check_interval_secs,
                autoupdate_command: patch.autoupdate_command.clone(),
                ..Default::default()
            })?;
        }

        let current = Self::user_settings_get(state, user_id).await?;
        let updated = current.apply_patch(patch);
        Self::user_settings_upsert(state, user_id, &updated).await?;
        if requeue_image_jobs {
            crate::db::repositories::image_jobs::requeue_failed_for_user(&state.db, user_id)
                .await?;
        }
        Ok(())
    }
}
