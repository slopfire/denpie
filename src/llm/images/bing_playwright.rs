//! Optional Bing Images discovery through the repository's Playwright install.
//!
//! The helper returns source URLs only. Image bytes are fetched and validated by
//! the normal Rust image downloader after this adapter has selected a URL.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::{ImageInput, RetrievedImage, download_candidates};

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(30);
const STDOUT_CAP: usize = 128 * 1024;
const STDERR_CAP: usize = 16 * 1024;
const CANDIDATE_CAP: usize = 32;
const QUERY_CHAR_CAP: usize = 512;

/// Run the optional Playwright Bing Images helper and return source image URLs.
pub async fn discover(query: &str) -> Result<Vec<String>, String> {
    if disabled_by_env() {
        return Err(
            "Bing Playwright image discovery is disabled by DENPIE_DISABLE_BING_PLAYWRIGHT"
                .to_string(),
        );
    }

    let query = query.trim();
    if query.is_empty() {
        return Err("Bing Playwright image discovery requires a non-empty query".to_string());
    }
    if query.chars().count() > QUERY_CHAR_CAP {
        return Err(format!(
            "Bing Playwright image query exceeds the {QUERY_CHAR_CAP}-character limit"
        ));
    }

    let script = helper_path();
    let binary = std::env::var_os("DENPIE_PLAYWRIGHT_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("node"));
    let args = helper_args(&script, query);

    let mut child = Command::new(&binary)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            format!(
                "Bing Playwright image discovery could not start Node ({:?}): {error}",
                binary
            )
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Bing Playwright helper stdout was not available".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Bing Playwright helper stderr was not available".to_string())?;

    let execution = async {
        let stdout_read = read_capped(stdout, STDOUT_CAP);
        let stderr_read = read_capped(stderr, STDERR_CAP);
        let (stdout, stderr, status) = tokio::join!(stdout_read, stderr_read, child.wait());
        let status = status.map_err(|error| format!("could not wait for Node: {error}"))?;
        let stdout = stdout.map_err(|error| format!("could not read Node stdout: {error}"))?;
        let stderr = stderr.map_err(|error| format!("could not read Node stderr: {error}"))?;
        Ok::<_, String>((status, stdout, stderr))
    };

    let (status, stdout, stderr) = tokio::time::timeout(DISCOVERY_TIMEOUT, execution)
        .await
        .map_err(|_| "Bing Playwright image discovery timed out after 30 seconds".to_string())??;

    if !status.success() {
        let detail = display_process_detail(&stderr, &stdout);
        return Err(format!(
            "Bing Playwright image discovery failed ({}): {detail}",
            status
        ));
    }

    parse_candidates(&stdout)
}

/// Discover Bing candidates and hand them to the shared bounded image downloader.
pub async fn retrieve(input: ImageInput<'_>) -> Option<RetrievedImage> {
    if input.image_query.trim().is_empty() {
        return None;
    }
    let urls = match discover(input.image_query).await {
        Ok(urls) => urls,
        Err(error) => {
            tracing::warn!(%error, "Bing Playwright image discovery failed");
            return None;
        }
    };
    download_candidates(&urls).await
}

/// Build the exact argv passed to Node. Keeping this separate makes it possible
/// to verify that query text is an argument, never shell source.
pub(crate) fn helper_args(script: &Path, query: &str) -> Vec<OsString> {
    vec![script.as_os_str().to_os_string(), OsString::from(query)]
}

fn helper_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("bing-image-search.mjs")
}

fn disabled_by_env() -> bool {
    disabled_value(
        std::env::var("DENPIE_DISABLE_BING_PLAYWRIGHT")
            .ok()
            .as_deref(),
    )
}

fn disabled_value(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

async fn read_capped<R>(reader: R, cap: usize) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut limited = reader.take((cap + 1) as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).await?;
    Ok(bytes)
}

fn display_process_detail(stderr: &[u8], stdout: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    "Node exited without an error message".to_string()
}

fn parse_candidates(stdout: &[u8]) -> Result<Vec<String>, String> {
    if stdout.len() > STDOUT_CAP {
        return Err(format!(
            "Bing Playwright helper output exceeded the {STDOUT_CAP}-byte limit"
        ));
    }

    let values: Vec<String> = serde_json::from_slice(stdout)
        .map_err(|error| format!("Bing Playwright helper returned invalid JSON: {error}"))?;
    let mut candidates = Vec::with_capacity(values.len().min(CANDIDATE_CAP));
    for value in values {
        let value = value.trim();
        let Ok(url) = url::Url::parse(value) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            continue;
        }
        let value = url.to_string();
        if !candidates.iter().any(|candidate| candidate == &value) {
            candidates.push(value);
        }
        if candidates.len() >= CANDIDATE_CAP {
            break;
        }
    }
    Ok(candidates)
}

#[cfg(test)]
mod tests {
    use super::{disabled_value, helper_args, parse_candidates};
    use std::path::Path;

    #[test]
    fn helper_argv_keeps_query_as_one_argument() {
        let args = helper_args(
            Path::new("scripts/bing-image-search.mjs"),
            "English prepositions; $(touch /tmp/should-not-run)",
        );
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "scripts/bing-image-search.mjs");
        assert_eq!(
            args[1],
            "English prepositions; $(touch /tmp/should-not-run)"
        );
    }

    #[test]
    fn disable_values_are_explicit_and_case_tolerant_for_common_values() {
        assert!(disabled_value(Some("1")));
        assert!(disabled_value(Some(" true ")));
        assert!(disabled_value(Some("YES")));
        assert!(!disabled_value(Some("0")));
        assert!(!disabled_value(Some("false")));
        assert!(!disabled_value(None));
    }

    #[test]
    fn parses_and_filters_bounded_url_candidates() {
        let output = br#"["https://example.test/a.png", "data:image/png;base64,abc", "not a URL", "https://example.test/a.png", "http://cdn.test/b.webp"]"#;
        assert_eq!(
            parse_candidates(output).unwrap(),
            ["https://example.test/a.png", "http://cdn.test/b.webp"]
        );
    }

    #[test]
    fn rejects_oversized_helper_output_before_parsing() {
        let output = vec![b' '; super::STDOUT_CAP + 1];
        let error = parse_candidates(&output).unwrap_err();
        assert!(error.contains("output exceeded"));
    }

    #[tokio::test]
    #[ignore = "live Playwright/Bing smoke; not for CI"]
    async fn live_playwright_discovers_image_candidates() {
        let urls = super::discover("diagram of in on at prepositions of place")
            .await
            .unwrap();
        assert!(!urls.is_empty());
        assert!(super::download_candidates(&urls).await.is_some());
    }
}
