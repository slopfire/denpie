use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tokio::time;
use tracing::{error, info, warn};

use crate::autoupdate::config::AutoupdateConfig;
use crate::autoupdate::github::latest_github_sha;
use crate::autoupdate::settings_io::{read_settings, short_sha, write_last_seen_sha};
use crate::autoupdate::status::write_status;
use crate::http_client;

pub fn spawn(settings_path: std::path::PathBuf) {
    tokio::spawn(async move {
        let client = http_client::shared();
        loop {
            match run_once(client, &settings_path).await {
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

pub(crate) enum CheckResult {
    Disabled,
    NoChange(Duration),
    Updated,
}

pub(crate) async fn run_once(
    client: &reqwest::Client,
    settings_path: &Path,
) -> Result<CheckResult, String> {
    let settings = read_settings(settings_path).await?;
    let config = AutoupdateConfig::from_settings(&settings);

    if !config.enabled || config.repo.is_empty() {
        return Ok(CheckResult::Disabled);
    }

    write_status(
        settings_path,
        "checking",
        &format!("Checking GitHub branch {}:{}", config.repo, config.branch),
        None,
    );
    let latest_sha = match latest_github_sha(client, &config.repo, &config.branch).await {
        Ok(sha) => sha,
        Err(err) => {
            write_status(
                settings_path,
                "failed",
                &format!("GitHub update check failed: {err}"),
                None,
            );
            return Err(err);
        }
    };
    if latest_sha.is_empty() {
        write_status(settings_path, "failed", "No commit SHA found", None);
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
        write_status(
            settings_path,
            "baseline",
            "Recorded server update baseline",
            Some(&latest_sha),
        );
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    if config.last_seen_sha == latest_sha {
        write_status(
            settings_path,
            "current",
            "Already up to date",
            Some(&latest_sha),
        );
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
        write_status(
            settings_path,
            "idle",
            "Update detected; default systemd updater handles scheduled install",
            Some(&latest_sha),
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
    write_status(
        settings_path,
        "running",
        "Running configured server update command",
        Some(&latest_sha),
    );

    let status = run_command(&config.command).await?;

    if !status.success() {
        error!(?status, "server update command failed");
        write_status(
            settings_path,
            "failed",
            &format!("Configured server update command failed with {status}"),
            Some(&latest_sha),
        );
        return Ok(CheckResult::NoChange(Duration::from_secs(
            config.check_interval_secs,
        )));
    }

    write_last_seen_sha(settings_path, &latest_sha).await?;
    Ok(CheckResult::Updated)
}

pub(crate) async fn run_command(command: &str) -> Result<std::process::ExitStatus, String> {
    Command::new("sh")
        .arg("-c")
        .arg(command)
        .status()
        .await
        .map_err(|err| format!("failed to spawn update command: {err}"))
}
