use serde_yaml::Value;

#[derive(Clone, Debug)]
pub struct AutoupdateConfig {
    pub enabled: bool,
    pub repo: String,
    pub branch: String,
    pub check_interval_secs: u64,
    pub command: String,
    pub last_seen_sha: String,
}

pub(crate) const DEFAULT_REPO: &str = "slopfire/denpie";

impl AutoupdateConfig {
    pub fn from_settings(settings: &Value) -> Self {
        Self {
            enabled: settings
                .get("autoupdate_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            repo: normalize_repo(
                settings
                    .get("autoupdate_repo")
                    .and_then(Value::as_str)
                    .unwrap_or(DEFAULT_REPO)
                    .trim(),
            ),
            branch: settings
                .get("autoupdate_branch")
                .and_then(Value::as_str)
                .unwrap_or("master")
                .trim()
                .to_string(),
            check_interval_secs: settings
                .get("autoupdate_check_interval_secs")
                .and_then(Value::as_u64)
                .unwrap_or(3600)
                .max(60),
            command: settings
                .get("autoupdate_command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            last_seen_sha: settings
                .get("autoupdate_last_seen_sha")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
        }
    }
}

pub(crate) fn normalize_repo(repo: &str) -> String {
    let mut value = repo.trim().to_string();
    for prefix in [
        "https://github.com/",
        "http://github.com/",
        "git@github.com:",
    ] {
        if let Some(stripped) = value.strip_prefix(prefix) {
            value = stripped.to_string();
            break;
        }
    }
    if value.starts_with("git@") {
        if let Some((_, path)) = value.split_once(':') {
            value = path.to_string();
        }
    }
    value = value
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string();
    value
}
