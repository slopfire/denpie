//! Reproducible metadata and safe display helpers for opt-in lab runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(crate) const MANIFEST_SCHEMA_VERSION: u32 = 1;
const RUNS_DIR: &str = "lab/runs";
const BASELINES_PATH: &str = "lab/runs/baselines.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RunManifest {
    pub(crate) schema_version: u32,
    pub(crate) bench: String,
    #[serde(default)]
    pub(crate) label: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) git_revision: Option<String>,
    pub(crate) git_dirty: Option<bool>,
    pub(crate) case_pack_path: String,
    pub(crate) case_pack_sha256: String,
    pub(crate) model: Option<String>,
    pub(crate) api_origin: Option<String>,
    pub(crate) compatibility: BTreeMap<String, Value>,
    pub(crate) execution: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryPolicy {
    Miss,
    Timeout,
}

impl RunManifest {
    pub(crate) fn new(
        bench: &str,
        case_pack_path: &str,
        model: Option<&str>,
        api_base: Option<&str>,
        compatibility: BTreeMap<String, Value>,
        execution: BTreeMap<String, Value>,
    ) -> Result<Self, String> {
        let (git_revision, git_dirty) = git_state();
        Ok(Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            bench: bench.to_string(),
            label: None,
            started_at: Utc::now(),
            git_revision,
            git_dirty,
            case_pack_path: case_pack_path.to_string(),
            case_pack_sha256: file_sha256(case_pack_path)?,
            model: model.map(str::to_string),
            api_origin: api_base.map(sanitize_endpoint),
            compatibility,
            execution,
        })
    }

    pub(crate) fn compatibility_error(&self, other: &Self) -> Option<String> {
        let mut differences = Vec::new();
        if self.schema_version != other.schema_version {
            differences.push(format!(
                "manifest schema {} vs {}",
                self.schema_version, other.schema_version
            ));
        }
        if self.bench != other.bench {
            differences.push(format!("bench {} vs {}", self.bench, other.bench));
        }
        if self.case_pack_sha256 != other.case_pack_sha256 {
            differences.push("case-pack content differs".to_string());
        }
        if self.model != other.model {
            differences.push(format!("model {:?} vs {:?}", self.model, other.model));
        }
        if self.api_origin != other.api_origin {
            differences.push(format!(
                "API origin {:?} vs {:?}",
                self.api_origin, other.api_origin
            ));
        }
        if self.compatibility != other.compatibility {
            differences.push("bench settings differ".to_string());
        }
        (!differences.is_empty()).then(|| differences.join(", "))
    }
}

pub(crate) async fn write_manifest(run_dir: &str, manifest: &RunManifest) -> Result<(), String> {
    let path = Path::new(run_dir).join("manifest.json");
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|error| format!("failed to serialize run manifest: {error}"))?;
    write_atomically(&path, json).await
}

pub(crate) fn load_scorecard_manifest(scorecard_path: &str) -> Result<RunManifest, String> {
    let scorecard = Path::new(scorecard_path);
    let path = scorecard
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("manifest.json");
    let json = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "scorecard `{scorecard_path}` has no readable run manifest `{}`: {error}",
            path.display()
        )
    })?;
    let manifest: RunManifest = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse run manifest `{}`: {error}", path.display()))?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "run manifest `{}` uses unsupported schema version {} (expected {})",
            path.display(),
            manifest.schema_version,
            MANIFEST_SCHEMA_VERSION
        ));
    }
    Ok(manifest)
}

pub(crate) fn load_run_manifest(run_dir: &str) -> Result<RunManifest, String> {
    let path = Path::new(run_dir).join("manifest.json");
    let json = std::fs::read_to_string(&path)
        .map_err(|error| format!("failed to read run manifest `{}`: {error}", path.display()))?;
    let manifest: RunManifest = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse run manifest `{}`: {error}", path.display()))?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "run manifest `{}` uses unsupported schema version {}",
            path.display(),
            manifest.schema_version
        ));
    }
    Ok(manifest)
}

pub(crate) fn validate_resume(
    run_dir: &str,
    expected: &RunManifest,
) -> Result<RunManifest, String> {
    let existing = load_run_manifest(run_dir)?;
    if let Some(reason) = existing.compatibility_error(expected) {
        return Err(format!("cannot resume incompatible lab run: {reason}"));
    }
    let mut existing_execution = existing.execution.clone();
    let mut expected_execution = expected.execution.clone();
    existing_execution.remove("concurrency");
    expected_execution.remove("concurrency");
    if existing_execution != expected_execution {
        return Err(
            "cannot resume lab run with different case selection, repeat count, or timeout"
                .to_string(),
        );
    }
    Ok(existing)
}

pub(crate) fn list_runs() -> Result<String, String> {
    let mut runs = run_directories()?;
    runs.reverse();
    let mut output = String::from("runs:\n");
    for path in runs.into_iter().take(50) {
        let manifest_path = path.join("manifest.json");
        let Ok(json) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<RunManifest>(&json) else {
            continue;
        };
        let label = manifest
            .label
            .as_deref()
            .map(|label| format!(" [{label}]"))
            .or_else(|| {
                std::fs::read_to_string(path.join("label.txt"))
                    .ok()
                    .map(|label| format!(" [{}]", label.trim()))
            })
            .unwrap_or_default();
        output.push_str(&format!(
            "  {}  {}  {}{}\n",
            path.display(),
            manifest.bench,
            manifest.started_at.to_rfc3339(),
            label
        ));
    }
    Ok(output)
}

pub(crate) fn show_run(selector: &str) -> Result<String, String> {
    let path = resolve_run(selector)?;
    let manifest_path = path.join("manifest.json");
    let json = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read `{}`: {error}", manifest_path.display()))?;
    let manifest: RunManifest = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse `{}`: {error}", manifest_path.display()))?;
    let pretty = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("failed to render run manifest: {error}"))?;
    Ok(format!("run: {}\n{pretty}\n", path.display()))
}

pub(crate) fn label_run(selector: &str, label: &str) -> Result<String, String> {
    validate_label(label)?;
    let path = resolve_run(selector)?;
    let manifest_path = path.join("manifest.json");
    let json = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("failed to read `{}`: {error}", manifest_path.display()))?;
    let mut manifest: RunManifest = serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse `{}`: {error}", manifest_path.display()))?;
    manifest.label = Some(label.to_string());
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("failed to serialize run manifest: {error}"))?;
    std::fs::write(&manifest_path, json)
        .map_err(|error| format!("failed to write `{}`: {error}", manifest_path.display()))?;
    let label_path = path.join("label.txt");
    std::fs::write(&label_path, format!("{label}\n"))
        .map_err(|error| format!("failed to write `{}`: {error}", label_path.display()))?;
    Ok(format!("label: {label}\nrun: {}\n", path.display()))
}

pub(crate) fn set_baseline(name: &str, selector: &str) -> Result<String, String> {
    validate_label(name)?;
    let run = resolve_run(selector)?;
    let mut baselines = load_baselines()?;
    baselines.insert(name.to_string(), run.display().to_string());
    std::fs::create_dir_all(RUNS_DIR)
        .map_err(|error| format!("failed to create `{RUNS_DIR}`: {error}"))?;
    let json = serde_json::to_string_pretty(&baselines)
        .map_err(|error| format!("failed to serialize baselines: {error}"))?;
    std::fs::write(BASELINES_PATH, json)
        .map_err(|error| format!("failed to write `{BASELINES_PATH}`: {error}"))?;
    Ok(format!("baseline: {name}\nrun: {}\n", run.display()))
}

pub(crate) fn show_baselines(name: Option<&str>) -> Result<String, String> {
    let baselines = load_baselines()?;
    if let Some(name) = name {
        let run = baselines
            .get(name)
            .ok_or_else(|| format!("no named baseline `{name}`"))?;
        return Ok(format!("baseline: {name}\nrun: {run}\n"));
    }
    let mut output = String::from("baselines:\n");
    for (name, run) in baselines {
        output.push_str(&format!("  {name}  {run}\n"));
    }
    Ok(output)
}

fn load_baselines() -> Result<BTreeMap<String, String>, String> {
    match std::fs::read_to_string(BASELINES_PATH) {
        Ok(json) => serde_json::from_str(&json)
            .map_err(|error| format!("failed to parse `{BASELINES_PATH}`: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(format!("failed to read `{BASELINES_PATH}`: {error}")),
    }
}

pub(crate) fn validate_label(label: &str) -> Result<(), String> {
    if label.is_empty()
        || !label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("run labels must contain only ASCII letters, digits, `-`, or `_`".to_string());
    }
    Ok(())
}

fn resolve_run(selector: &str) -> Result<PathBuf, String> {
    if selector != "latest" {
        let direct = PathBuf::from(selector);
        if direct.join("manifest.json").is_file() {
            return Ok(direct);
        }
        for path in run_directories()? {
            let matches_name = path.file_name().and_then(|name| name.to_str()) == Some(selector);
            let matches_label = std::fs::read_to_string(path.join("label.txt"))
                .ok()
                .is_some_and(|label| label.trim() == selector)
                || std::fs::read_to_string(path.join("manifest.json"))
                    .ok()
                    .and_then(|json| serde_json::from_str::<RunManifest>(&json).ok())
                    .and_then(|manifest| manifest.label)
                    .is_some_and(|label| label == selector);
            if matches_name || matches_label {
                return Ok(path);
            }
        }
        return Err(format!("no lab run matches `{selector}`"));
    }
    run_directories()?
        .into_iter()
        .rev()
        .find(|path| path.join("manifest.json").is_file())
        .ok_or_else(|| format!("no manifested lab runs found under `{RUNS_DIR}`"))
}

fn run_directories() -> Result<Vec<PathBuf>, String> {
    let root = Path::new(RUNS_DIR);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = std::fs::read_dir(root)
        .map_err(|error| format!("failed to read `{RUNS_DIR}`: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

pub(crate) fn sanitize_endpoint(raw: &str) -> String {
    let Ok(url) = url::Url::parse(raw) else {
        return "[invalid endpoint]".to_string();
    };
    let Some(host) = url.host_str() else {
        return "[invalid endpoint]".to_string();
    };
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    format!("{}://{host}{port}", url.scheme())
}

pub(crate) fn redact_provider_error(error: &str, api_key: &str, api_base: &str) -> String {
    let mut redacted = error.to_string();
    if !api_key.is_empty() {
        redacted = redacted.replace(api_key, "[redacted]");
    }
    if !api_base.is_empty() {
        redacted = redacted.replace(api_base, &sanitize_endpoint(api_base));
    }
    redact_url_credentials(&redacted)
}

fn redact_url_credentials(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("http://").or_else(|| rest.find("https://")) {
        output.push_str(&rest[..start]);
        let url_and_rest = &rest[start..];
        let end = url_and_rest
            .find(|character: char| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ')' | ']' | '}')
            })
            .unwrap_or(url_and_rest.len());
        let candidate = &url_and_rest[..end];
        if let Ok(mut url) = url::Url::parse(candidate) {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            if url.query().is_some() {
                url.set_query(Some("[redacted]"));
            }
            output.push_str(url.as_str());
        } else {
            output.push_str(candidate);
        }
        rest = &url_and_rest[end..];
    }
    output.push_str(rest);
    output
}

fn file_sha256(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to hash case pack `{path}`: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn git_state() -> (Option<String>, Option<bool>) {
    let revision = git_output(["rev-parse", "HEAD"]);
    let dirty = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| !output.stdout.is_empty());
    (revision, dirty)
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|output| output.trim().to_string())
        .filter(|output| !output.is_empty())
}

async fn write_atomically(path: &Path, contents: String) -> Result<(), String> {
    let temporary_path = PathBuf::from(format!("{}.tmp", path.display()));
    tokio::fs::write(&temporary_path, contents)
        .await
        .map_err(|error| format!("failed to write `{}`: {error}", temporary_path.display()))?;
    tokio::fs::rename(&temporary_path, path)
        .await
        .map_err(|error| format!("failed to checkpoint `{}`: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_display_drops_path_userinfo_and_query() {
        assert_eq!(
            sanitize_endpoint("https://alice:secret@example.test:8443/v1?api_key=secret"),
            "https://example.test:8443"
        );
    }

    #[test]
    fn provider_errors_redact_known_and_embedded_url_secrets() {
        let error = redact_provider_error(
            "key sk-live failed at https://user:pass@example.test/v1?token=hunter2",
            "sk-live",
            "https://example.test/v1?token=hunter2",
        );
        assert!(!error.contains("sk-live"));
        assert!(!error.contains("hunter2"));
        assert!(!error.contains("pass"));
        assert!(error.contains("[redacted]"));
    }

    #[test]
    fn compatibility_reports_case_pack_and_setting_drift() {
        let base = RunManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            bench: "images".to_string(),
            label: None,
            started_at: Utc::now(),
            git_revision: None,
            git_dirty: None,
            case_pack_path: "gold.json".to_string(),
            case_pack_sha256: "aaa".to_string(),
            model: None,
            api_origin: None,
            compatibility: BTreeMap::from([(
                "strategies".to_string(),
                serde_json::json!(["bing_html"]),
            )]),
            execution: BTreeMap::new(),
        };
        let mut changed = base.clone();
        changed.case_pack_sha256 = "bbb".to_string();
        changed.compatibility.insert(
            "strategies".to_string(),
            serde_json::json!(["ddgs_text_og"]),
        );

        let error = base
            .compatibility_error(&changed)
            .expect("drift is incompatible");
        assert!(error.contains("case-pack content differs"));
        assert!(error.contains("bench settings differ"));
    }

    #[test]
    fn resume_allows_concurrency_change_but_rejects_repeat_change() {
        let existing = RunManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            bench: "images".to_string(),
            label: None,
            started_at: Utc::now(),
            git_revision: None,
            git_dirty: None,
            case_pack_path: "gold.json".to_string(),
            case_pack_sha256: "aaa".to_string(),
            model: None,
            api_origin: None,
            compatibility: BTreeMap::new(),
            execution: BTreeMap::from([
                ("concurrency".to_string(), serde_json::json!(2)),
                ("repeat".to_string(), serde_json::json!(1)),
            ]),
        };
        let root = std::env::temp_dir().join(format!(
            "denpie-lab-resume-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).expect("resume fixture directory");
        std::fs::write(
            root.join("manifest.json"),
            serde_json::to_string(&existing).expect("serialize manifest"),
        )
        .expect("write manifest");

        let mut expected = existing.clone();
        expected
            .execution
            .insert("concurrency".to_string(), serde_json::json!(5));
        validate_resume(root.to_str().expect("UTF-8 path"), &expected)
            .expect("concurrency is not a job identity field");
        expected
            .execution
            .insert("repeat".to_string(), serde_json::json!(2));
        assert!(
            validate_resume(root.to_str().expect("UTF-8 path"), &expected)
                .unwrap_err()
                .contains("different case selection")
        );
        std::fs::remove_dir_all(root).expect("resume fixture removed");
    }
}
