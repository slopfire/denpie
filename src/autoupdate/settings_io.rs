use serde_yaml::{Mapping, Value};
use std::path::Path;

pub(crate) async fn read_settings(path: &Path) -> Result<Value, String> {
    let settings_str = tokio::fs::read_to_string(path).await.unwrap_or_default();
    let mut settings: Value =
        serde_yaml::from_str(&settings_str).unwrap_or_else(|_| Value::Mapping(Mapping::new()));
    if !settings.is_mapping() {
        settings = Value::Mapping(Mapping::new());
    }
    Ok(settings)
}

pub(crate) async fn write_last_seen_sha(path: &Path, sha: &str) -> Result<(), String> {
    let mut settings = read_settings(path).await?;
    if let Value::Mapping(ref mut map) = settings {
        map.insert(
            Value::String("autoupdate_last_seen_sha".to_string()),
            Value::String(sha.to_string()),
        );
    }
    let out = serde_yaml::to_string(&settings)
        .map_err(|err| format!("settings serialization failed: {err}"))?;
    tokio::fs::write(path, out)
        .await
        .map_err(|err| format!("settings write failed: {err}"))
}

pub(crate) fn short_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}
