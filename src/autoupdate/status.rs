use serde::Serialize;
use std::path::Path;

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

pub(crate) fn write_status(
    settings_path: &Path,
    phase: &str,
    message: &str,
    target_sha: Option<&str>,
) {
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
