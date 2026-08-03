//! Local page scraping via the [Scrapling](https://github.com/D4Vinci/Scrapling) CLI.
//!
//! Scrapling is the main option for turning linked web pages into clean Markdown for
//! grounding documents. It runs as an optional external process so Denpie stays a
//! pure Rust binary when Scrapling is not installed.
//!
//! Install (optional):
//! ```text
//! pip install "scrapling[fetchers,shell]"
//! ```
//!
//! Override the binary with `DENPIE_SCRAPLING_BIN` (default: `scrapling` on `PATH`).
//! Disable entirely with `DENPIE_DISABLE_SCRAPLING=1`.

use std::{
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
    time::Duration,
};

use rand::{Rng, distributions::Alphanumeric};
use tokio::process::Command;
use url::Url;

use crate::error::{AppError, AppResult};

/// Soft cap on Markdown returned from Scrapling (matches link fetch byte budget).
const OUTPUT_CHAR_CAP: usize = 2 * 1024 * 1024;
const SCRAPE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScraplingStatus {
    Available,
    Missing,
    Disabled,
}

/// Whether Scrapling can be used on this host (cached after first probe).
pub fn status() -> ScraplingStatus {
    static STATUS: OnceLock<ScraplingStatus> = OnceLock::new();
    *STATUS.get_or_init(probe_status)
}

fn probe_status() -> ScraplingStatus {
    if disabled_by_env() {
        return ScraplingStatus::Disabled;
    }
    let bin = binary_path();
    match std::process::Command::new(&bin)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => ScraplingStatus::Available,
        Ok(_) | Err(_) => ScraplingStatus::Missing,
    }
}

fn disabled_by_env() -> bool {
    matches!(
        std::env::var("DENPIE_DISABLE_SCRAPLING").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn binary_path() -> PathBuf {
    std::env::var_os("DENPIE_SCRAPLING_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("scrapling"))
}

/// Scrape a public HTTP(S) URL to AI-oriented Markdown via Scrapling.
///
/// Returns `Ok(None)` when Scrapling is disabled or not installed so callers can
/// fall back. Returns `Err` for invalid URLs, private targets, or scrape failures
/// after Scrapling was confirmed available.
pub async fn scrape_url(url: &str) -> AppResult<Option<String>> {
    match status() {
        ScraplingStatus::Disabled | ScraplingStatus::Missing => return Ok(None),
        ScraplingStatus::Available => {}
    }

    let parsed = validate_public_scrape_url(url).await?;
    let markdown = run_extract(&parsed).await?;
    if markdown.trim().is_empty() {
        return Err(AppError::Validation(
            "Scrapling returned no document content".to_string(),
        ));
    }
    Ok(Some(markdown))
}

/// Build the CLI argv used for extraction (unit-tested without running Scrapling).
pub(crate) fn extract_args(url: &str, output: &Path) -> Vec<String> {
    vec![
        "extract".into(),
        "get".into(),
        url.into(),
        output.display().to_string(),
        "--ai-targeted".into(),
        "--timeout".into(),
        "60".into(),
        "--impersonate".into(),
        "chrome".into(),
    ]
}

async fn run_extract(url: &Url) -> AppResult<String> {
    let out_path = temp_markdown_path();
    let args = extract_args(url.as_str(), &out_path);
    let bin = binary_path();

    let output = Command::new(&bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let result = tokio::time::timeout(SCRAPE_TIMEOUT, output)
        .await
        .map_err(|_| {
            AppError::Validation("Scrapling timed out while scraping this URL".to_string())
        })?
        .map_err(|err| AppError::Validation(format!("Failed to run Scrapling ({bin:?}): {err}")))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("unknown error");
        let _ = tokio::fs::remove_file(&out_path).await;
        return Err(AppError::Validation(format!(
            "Scrapling could not scrape this document: {detail}"
        )));
    }

    let markdown = tokio::fs::read_to_string(&out_path)
        .await
        .map_err(|err| AppError::Validation(format!("Scrapling wrote no output file: {err}")))?;
    let _ = tokio::fs::remove_file(&out_path).await;

    let trimmed = if markdown.len() > OUTPUT_CHAR_CAP {
        markdown.chars().take(OUTPUT_CHAR_CAP).collect()
    } else {
        markdown
    };
    Ok(trimmed.trim().to_string())
}

fn temp_markdown_path() -> PathBuf {
    let token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    std::env::temp_dir().join(format!("denpie-scrapling-{token}.md"))
}

/// Reject private/local/credentialed targets before shelling out to Scrapling.
async fn validate_public_scrape_url(value: &str) -> AppResult<Url> {
    let url = Url::parse(value.trim())
        .map_err(|_| AppError::Validation("Invalid document URL".to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(AppError::Validation(
            "Document URLs must be credential-free HTTP or HTTPS URLs".to_string(),
        ));
    }

    let host = url
        .host_str()
        .ok_or_else(|| AppError::Validation("Invalid document URL".to_string()))?;
    let port = url.port_or_known_default().unwrap_or(80);
    let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| {
                AppError::Validation("Document URL host could not be resolved".to_string())
            })?
            .collect()
    };

    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_public_address(address.ip()))
    {
        return Err(AppError::Validation(
            "Document URLs may not target private network addresses".to_string(),
        ));
    }
    Ok(url)
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_multicast()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && first != 0
                && !(first == 100 && (64..=127).contains(&second))
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            let ipv4_compatible = octets[..12].iter().all(|byte| *byte == 0);
            let ipv4_mapped = octets[..10].iter().all(|byte| *byte == 0)
                && octets[10] == 0xff
                && octets[11] == 0xff;
            if ipv4_compatible || ipv4_mapped {
                return is_public_address(IpAddr::from([
                    octets[12], octets[13], octets[14], octets[15],
                ]));
            }
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && (octets[0] & 0xfe) != 0xfc // unique local
                && (octets[0] != 0xfe || (octets[1] & 0xc0) != 0x80) // link-local
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extract_args_request_ai_targeted_markdown() {
        let args = extract_args("https://example.com/guide", Path::new("/tmp/out.md"));
        assert_eq!(
            args,
            vec![
                "extract",
                "get",
                "https://example.com/guide",
                "/tmp/out.md",
                "--ai-targeted",
                "--timeout",
                "60",
                "--impersonate",
                "chrome",
            ]
        );
    }

    #[test]
    fn private_v4_addresses_are_rejected() {
        assert!(!is_public_address("127.0.0.1".parse().unwrap()));
        assert!(!is_public_address("10.0.0.1".parse().unwrap()));
        assert!(!is_public_address("192.168.1.1".parse().unwrap()));
        assert!(!is_public_address("169.254.1.1".parse().unwrap()));
        assert!(!is_public_address("100.64.0.1".parse().unwrap()));
        assert!(is_public_address("8.8.8.8".parse().unwrap()));
    }

    #[tokio::test]
    async fn credentialed_urls_are_rejected() {
        let err = validate_public_scrape_url("https://user:pass@example.com/doc")
            .await
            .unwrap_err();
        assert!(err.message().contains("credential-free HTTP or HTTPS URLs"));
    }

    #[tokio::test]
    async fn loopback_urls_are_rejected() {
        let err = validate_public_scrape_url("http://127.0.0.1/secret")
            .await
            .unwrap_err();
        assert!(err.message().contains("private network"));
    }
}
