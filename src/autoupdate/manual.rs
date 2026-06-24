use std::path::Path;
use tracing::info;

use crate::autoupdate::config::AutoupdateConfig;
use crate::autoupdate::github::latest_github_sha;
use crate::autoupdate::runner::run_command;
use crate::autoupdate::settings_io::{read_settings, short_sha, write_last_seen_sha};
use crate::autoupdate::status::write_status;
use crate::autoupdate::systemd::start_systemd_updater;
use crate::http_client;

#[derive(Debug)]
pub struct ManualCheckResult {
    pub message: String,
    pub should_exit_for_restart: bool,
    pub update_started: bool,
    pub target_sha: Option<String>,
}

pub async fn trigger_manual(settings_path: &Path) -> Result<ManualCheckResult, String> {
    let client = http_client::shared();
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
        let message = start_systemd_updater(settings_path, &config, &latest_sha).await?;
        return Ok(ManualCheckResult {
            message,
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

    let status = run_command(&config.command).await?;

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
