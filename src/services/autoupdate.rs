use std::path::Path;

use crate::autoupdate;

pub use autoupdate::UpdateStatus;

#[derive(Clone, Copy, Debug, Default)]
pub struct AutoupdateService;

impl AutoupdateService {
    pub async fn trigger_manual(
        settings_path: &Path,
    ) -> Result<autoupdate::ManualCheckResult, String> {
        autoupdate::trigger_manual(settings_path).await
    }

    pub fn read_status(settings_path: &Path) -> autoupdate::UpdateStatus {
        autoupdate::read_status(settings_path)
    }
}
