use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tokio::time;
use tracing::{error, info, warn};

#[derive(Clone, Debug)]
pub struct AutoupdateConfig {
    pub enabled: bool,
    pub repo: String,
    pub branch: String,
    pub check_interval_secs: u64,
    pub command: String,
    pub last_seen_sha: String,
}

impl AutoupdateConfig {
    pub fn from_settings(settings: &Value) -> Self {
        Self {
            enabled: settings
                .get("autoupdate_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            repo: settings
                .get("autoupdate_repo")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string(),
            branch: settings
                .get("autoupdate_branch")
                .and_then(Value::as_str)
                .unwrap_or("main")
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

#[derive(Deserialize)]
struct GithubCommitRes {
    sha: String,
}

pub fn spawn(settings_path: std::path::PathBuf) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        loop {
            match run_once(&client, &settings_path).await {
                Ok(CheckResult::Disabled) => time::sleep(Duration::from_secs(300)).await,
                Ok(CheckResult::NoChange(interval)) => time::sleep(interval).await,
                Ok(CheckResult::Updated) => {
                    warn!("autoupdate command succeeded; exiting for supervisor restart");
                    std::process::exit(75);
                }
                Err(err) => {
                    warn!("autoupdate check failed: {err}");
                    time::sleep(Duration::from_secs(300)).await;
                }
            }
        }
    });
}

enum CheckResult {
    Disabled,
    NoChange(Duration),
    Updated,
}

async fn run_once(client: &reqwest::Client, settings_path: &Path) -> Result<CheckResult, String> {
    let settings = read_settings(settings_path).await?;
    let config = AutoupdateConfig::from_settings(&settings);

    if !config.enabled || config.repo.is_empty() {
        return Ok(CheckResult::Disabled);
    }

    let latest_sha = latest_github_sha(client, &config.repo, &config.branch).await?;
    if latest_sha.is_empty() {
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    if config.last_seen_sha.is_empty() {
        write_last_seen_sha(settings_path, &latest_sha).await?;
        info!(
            repo = %config.repo,
            branch = %config.branch,
            sha = %short_sha(&latest_sha),
            "autoupdate baseline recorded"
        );
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    if config.last_seen_sha == latest_sha {
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    if config.command.is_empty() {
        warn!(
            repo = %config.repo,
            branch = %config.branch,
            old_sha = %short_sha(&config.last_seen_sha),
            new_sha = %short_sha(&latest_sha),
            "autoupdate change detected, but command is empty"
        );
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    info!(
        repo = %config.repo,
        branch = %config.branch,
        old_sha = %short_sha(&config.last_seen_sha),
        new_sha = %short_sha(&latest_sha),
        "autoupdate change detected; running command"
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&config.command)
        .status()
        .await
        .map_err(|err| format!("failed to spawn update command: {err}"))?;

    if !status.success() {
        error!(?status, "autoupdate command failed");
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    write_last_seen_sha(settings_path, &latest_sha).await?;
    Ok(CheckResult::Updated)
}

async fn latest_github_sha(
    client: &reqwest::Client,
    repo: &str,
    branch: &str,
) -> Result<String, String> {
    let repo = repo.trim_matches('/');
    let branch = if branch.trim().is_empty() {
        "main"
    } else {
        branch.trim()
    };
    let url = format!("https://api.github.com/repos/{repo}/commits/{branch}");
    let res = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "dailytipdraft-autoupdate")
        .send()
        .await
        .map_err(|err| format!("github request failed: {err}"))?;

    if !res.status().is_success() {
        return Err(format!("github returned {}", res.status()));
    }

    let body: GithubCommitRes = res
        .json()
        .await
        .map_err(|err| format!("github response parse failed: {err}"))?;
    Ok(body.sha)
}

async fn read_settings(path: &Path) -> Result<Value, String> {
    let settings_str = tokio::fs::read_to_string(path).await.unwrap_or_default();
    let mut settings: Value =
        serde_yaml::from_str(&settings_str).unwrap_or_else(|_| Value::Mapping(Mapping::new()));
    if !settings.is_mapping() {
        settings = Value::Mapping(Mapping::new());
    }
    Ok(settings)
}

async fn write_last_seen_sha(path: &Path, sha: &str) -> Result<(), String> {
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

fn short_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}
