use serde::{Deserialize, Serialize};
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

const DEFAULT_REPO: &str = "slopfire/denpie";
const DEFAULT_SYSTEMD_UPDATE_SERVICE: &str = "denpie-autoupdate.service";

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
                    warn!("server update command succeeded; exiting for supervisor restart");
                    std::process::exit(75);
                }
                Err(err) => {
                    warn!("server update check failed: {err}");
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

#[derive(Debug)]
pub struct ManualCheckResult {
    pub message: String,
    pub should_exit_for_restart: bool,
    pub update_started: bool,
    pub target_sha: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    pub phase: String,
    pub message: String,
    pub target_sha: String,
    pub updated_at: String,
}

pub fn read_status(settings_path: &Path) -> UpdateStatus {
    let status_path = settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("autoupdate")
        .join("status");
    let status = std::fs::read_to_string(status_path).unwrap_or_default();
    let mut phase = String::new();
    let mut message = String::new();
    let mut target_sha = String::new();
    let mut updated_at = String::new();

    for line in status.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "phase" => phase = value.to_string(),
            "message" => message = value.to_string(),
            "target_sha" => target_sha = value.to_string(),
            "updated_at" => updated_at = value.to_string(),
            _ => {}
        }
    }

    UpdateStatus {
        phase: if phase.is_empty() {
            "unknown".to_string()
        } else {
            phase
        },
        message,
        target_sha,
        updated_at,
    }
}

pub fn write_status(settings_path: &Path, phase: &str, message: &str, target_sha: Option<&str>) {
    let status_dir = settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("autoupdate");
    if std::fs::create_dir_all(&status_dir).is_err() {
        return;
    }
    let status_path = status_dir.join("status");
    let tmp_path = status_dir.join(format!("status.tmp.{}", std::process::id()));
    let clean_message = message.replace(['\r', '\n'], " ");
    let content = format!(
        "phase={phase}\nmessage={clean_message}\ntarget_sha={}\nupdated_at={}\n",
        target_sha.unwrap_or_default(),
        chrono::Utc::now().to_rfc3339()
    );
    if std::fs::write(&tmp_path, content).is_ok() {
        let _ = std::fs::rename(tmp_path, status_path);
    }
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
            "server update baseline recorded"
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
            "server update detected, but command is empty"
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
        "server update detected; running command"
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&config.command)
        .status()
        .await
        .map_err(|err| format!("failed to spawn update command: {err}"))?;

    if !status.success() {
        error!(?status, "server update command failed");
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    write_last_seen_sha(settings_path, &latest_sha).await?;
    Ok(CheckResult::Updated)
}

pub async fn trigger_manual(settings_path: &Path) -> Result<ManualCheckResult, String> {
    let client = reqwest::Client::new();
    let settings = read_settings(settings_path).await?;
    let config = AutoupdateConfig::from_settings(&settings);

    if !config.enabled {
        write_status(
            settings_path,
            "disabled",
            "Server self-updates disabled",
            None,
        );
        return Ok(ManualCheckResult {
            message: "Server self-updates disabled".to_string(),
            should_exit_for_restart: false,
            update_started: false,
            target_sha: None,
        });
    }
    if config.repo.is_empty() {
        write_status(settings_path, "invalid", "Server update repo empty", None);
        return Ok(ManualCheckResult {
            message: "Server update repo empty".to_string(),
            should_exit_for_restart: false,
            update_started: false,
            target_sha: None,
        });
    }

    let latest_sha = latest_github_sha(&client, &config.repo, &config.branch).await?;
    if latest_sha.is_empty() {
        write_status(settings_path, "failed", "No commit SHA found", None);
        return Ok(ManualCheckResult {
            message: "No commit SHA found".to_string(),
            should_exit_for_restart: false,
            update_started: false,
            target_sha: None,
        });
    }

    if config.last_seen_sha.is_empty() {
        write_last_seen_sha(settings_path, &latest_sha).await?;
        write_status(
            settings_path,
            "baseline",
            "Recorded server update baseline",
            Some(&latest_sha),
        );
        return Ok(ManualCheckResult {
            message: format!("Recorded baseline {}", short_sha(&latest_sha)),
            should_exit_for_restart: false,
            update_started: false,
            target_sha: Some(latest_sha),
        });
    }

    if config.last_seen_sha == latest_sha {
        write_status(
            settings_path,
            "current",
            "Already up to date",
            Some(&latest_sha),
        );
        return Ok(ManualCheckResult {
            message: format!("Already up to date at {}", short_sha(&latest_sha)),
            should_exit_for_restart: false,
            update_started: false,
            target_sha: Some(latest_sha),
        });
    }

    if config.command.is_empty() {
        let service = std::env::var("DENPIE_AUTOUPDATE_SERVICE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SYSTEMD_UPDATE_SERVICE.to_string());
        info!(
            repo = %config.repo,
            branch = %config.branch,
            old_sha = %short_sha(&config.last_seen_sha),
            new_sha = %short_sha(&latest_sha),
            service = %service,
            "manual server update triggered; starting default systemd updater"
        );
        write_status(
            settings_path,
            "starting",
            "Starting default server updater",
            Some(&latest_sha),
        );

        let load_state = Command::new("systemctl")
            .arg("show")
            .arg(&service)
            .arg("--property=LoadState")
            .arg("--value")
            .output()
            .await
            .map_err(|err| {
                format!(
                    "failed to inspect default server updater {service}: {err}; set autoupdate_command for custom installs"
                )
            })?;
        let load_state_text = String::from_utf8_lossy(&load_state.stdout)
            .trim()
            .to_string();
        if !load_state.status.success() || load_state_text != "loaded" {
            write_status(
                settings_path,
                "failed",
                &format!("Default updater {service} is not installed"),
                Some(&latest_sha),
            );
            return Err(format!(
                "no server update runner configured: default updater {service} is not installed; set autoupdate_command for this install or install the systemd updater"
            ));
        }

        let start_output = Command::new("systemctl")
            .arg("start")
            .arg("--no-block")
            .arg(&service)
            .output()
            .await
            .map_err(|err| {
                format!(
                    "failed to start default server updater {service}: {err}; set autoupdate_command for custom installs"
                )
            })?;

        if !start_output.status.success() {
            let stderr = String::from_utf8_lossy(&start_output.stderr)
                .trim()
                .to_string();
            let stdout = String::from_utf8_lossy(&start_output.stdout)
                .trim()
                .to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                start_output.status.to_string()
            };
            if detail.contains("Interactive authentication required") {
                write_status(
                    settings_path,
                    "failed",
                    &format!("Default updater {service} needs permission for the web service user"),
                    Some(&latest_sha),
                );
                return Err(format!(
                    "default server updater {service} needs permission for the web service user; rerun ./install.sh to install the polkit rule, or set autoupdate_command for custom installs"
                ));
            }
            write_status(
                settings_path,
                "failed",
                &format!("Default updater {service} failed: {detail}"),
                Some(&latest_sha),
            );
            return Err(format!(
                "default server updater {service} failed: {detail}; set autoupdate_command for custom installs"
            ));
        }

        write_status(
            settings_path,
            "queued",
            "Default server updater started",
            Some(&latest_sha),
        );
        return Ok(ManualCheckResult {
            message: format!(
                "Started updater for {} -> {}",
                short_sha(&config.last_seen_sha),
                short_sha(&latest_sha)
            ),
            should_exit_for_restart: false,
            update_started: true,
            target_sha: Some(latest_sha),
        });
    }

    info!(
        repo = %config.repo,
        branch = %config.branch,
        old_sha = %short_sha(&config.last_seen_sha),
        new_sha = %short_sha(&latest_sha),
        "manual server update triggered; running command"
    );
    write_status(
        settings_path,
        "running",
        "Running configured server update command",
        Some(&latest_sha),
    );

    let status = Command::new("sh")
        .arg("-c")
        .arg(&config.command)
        .status()
        .await
        .map_err(|err| format!("failed to spawn update command: {err}"))?;

    if !status.success() {
        write_status(
            settings_path,
            "failed",
            &format!("Configured server update command failed with {status}"),
            Some(&latest_sha),
        );
        return Err(format!("server update command failed with {status}"));
    }

    write_last_seen_sha(settings_path, &latest_sha).await?;
    write_status(
        settings_path,
        "installed",
        "Installed update; restarting server",
        Some(&latest_sha),
    );
    Ok(ManualCheckResult {
        message: format!("Installed update {}", short_sha(&latest_sha)),
        should_exit_for_restart: true,
        update_started: false,
        target_sha: Some(latest_sha),
    })
}

async fn latest_github_sha(
    client: &reqwest::Client,
    repo: &str,
    branch: &str,
) -> Result<String, String> {
    let repo = normalize_repo(repo);
    let repo = repo.trim_matches('/');
    let branch = if branch.trim().is_empty() {
        "master"
    } else {
        branch.trim()
    };
    let url = format!("https://api.github.com/repos/{repo}/commits/{branch}");
    let res = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "denpie-autoupdate")
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

fn normalize_repo(repo: &str) -> String {
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
