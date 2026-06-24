use serde::Deserialize;
use std::time::Duration;
use tokio::time::timeout;

use crate::autoupdate::config::normalize_repo;

const GITHUB_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct GithubCommitRes {
    sha: String,
}

pub(crate) async fn latest_github_sha(
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
    let res = timeout(
        GITHUB_CHECK_TIMEOUT,
        client
            .get(url)
            .header(reqwest::header::USER_AGENT, "denpie-autoupdate")
            .send(),
    )
    .await
    .map_err(|_| "github request timed out".to_string())?
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
