use std::path::Path;
use tokio::process::Command;
use tracing::info;

use crate::autoupdate::config::AutoupdateConfig;
use crate::autoupdate::settings_io::short_sha;
use crate::autoupdate::status::write_status;

pub(crate) const DEFAULT_SYSTEMD_UPDATE_SERVICE: &str = "denpie-autoupdate.service";

pub(crate) async fn start_systemd_updater(
    settings_path: &Path,
    config: &AutoupdateConfig,
    latest_sha: &str,
) -> Result<String, String> {
    let service = std::env::var("DENPIE_AUTOUPDATE_SERVICE")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SYSTEMD_UPDATE_SERVICE.to_string());
    info!(
        repo = %config.repo,
        branch = %config.branch,
        old_sha = %short_sha(&config.last_seen_sha),
        new_sha = %short_sha(latest_sha),
        service = %service,
        "manual server update triggered; starting default systemd updater"
    );
    write_status(
        settings_path,
        "starting",
        "Starting default server updater",
        Some(latest_sha),
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
            Some(latest_sha),
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
                Some(latest_sha),
            );
            return Err(format!(
                "default server updater {service} needs permission for the web service user; rerun ./install.sh to install the polkit rule, or set autoupdate_command for custom installs"
            ));
        }
        write_status(
            settings_path,
            "failed",
            &format!("Default updater {service} failed: {detail}"),
            Some(latest_sha),
        );
        return Err(format!(
            "default server updater {service} failed: {detail}; set autoupdate_command for custom installs"
        ));
    }

    write_status(
        settings_path,
        "queued",
        "Default server updater started",
        Some(latest_sha),
    );
    Ok(format!(
        "Started updater for {} -> {}",
        short_sha(&config.last_seen_sha),
        short_sha(latest_sha)
    ))
}
