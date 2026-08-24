//! Live image bake-off runner. The dry-run path is CI-safe; the live path
//! deliberately calls production `retrieve_image` and therefore needs network.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::domain::grounding::ImageStrategy;
use crate::lab::artifacts::{RetryPolicy, RunManifest, validate_resume, write_manifest};
use crate::lab::cases::{ImageCase, load_image_cases};
use crate::llm::{ImageInput, ReasoningConfig, RetrievedImage};

pub(crate) const DEFAULT_CONCURRENCY: usize = 5;
const RETRIEVE_TIMEOUT: Duration = Duration::from_secs(90);

pub(crate) struct ImageRunConfig<'a> {
    pub(crate) cases_path: &'a str,
    pub(crate) strategies: &'a [ImageStrategy],
    pub(crate) dry_run: bool,
    pub(crate) concurrency: usize,
    pub(crate) case_ids: &'a [u64],
    pub(crate) tags: &'a [String],
    pub(crate) repeat: usize,
    pub(crate) label: Option<&'a str>,
    pub(crate) resume: Option<&'a str>,
    pub(crate) retry: Option<RetryPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScorecardRow {
    pub(crate) case_id: u64,
    #[serde(default = "default_repeat_index")]
    pub(crate) repeat_index: usize,
    pub(crate) strategy: String,
    pub(crate) search_or_download: String,
    pub(crate) kind: String,
    pub(crate) bytes: usize,
    pub(crate) mime_type: Option<String>,
    pub(crate) extension: Option<String>,
    pub(crate) elapsed_ms: u64,
    #[serde(default)]
    pub(crate) queue_ms: u64,
    #[serde(default)]
    pub(crate) failure_stage: Option<String>,
    pub(crate) visual: String,
    pub(crate) expected: String,
}

pub(crate) async fn run_images(
    config: &ImageRunConfig<'_>,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let cases_path = config.cases_path;
    let strategies = config.strategies;
    let dry_run = config.dry_run;
    let concurrency = config.concurrency;
    let case_ids = config.case_ids;
    let tags = config.tags;
    let repeat = config.repeat;
    let label = config.label;
    let resume = config.resume;
    let retry = config.retry;
    let cases = select_cases(load_image_cases(cases_path)?, case_ids, tags)?;

    if dry_run {
        write!(
            stdout,
            "{}",
            dry_run_plan(cases_path, strategies, &cases, concurrency, repeat)
        )
        .map_err(|error| format!("failed to print dry-run plan: {error}"))?;
        return Ok(());
    }

    let compatibility = BTreeMap::from([(
        "strategies".to_string(),
        serde_json::json!(
            strategies
                .iter()
                .map(|strategy| strategy.as_str())
                .collect::<Vec<_>>()
        ),
    )]);
    let execution = BTreeMap::from([
        ("concurrency".to_string(), serde_json::json!(concurrency)),
        ("repeat".to_string(), serde_json::json!(repeat)),
        ("case_ids".to_string(), serde_json::json!(case_ids)),
        ("tags".to_string(), serde_json::json!(tags)),
        (
            "timeout_seconds".to_string(),
            serde_json::json!(RETRIEVE_TIMEOUT.as_secs()),
        ),
    ]);
    let mut manifest =
        RunManifest::new("images", cases_path, None, None, compatibility, execution)?;
    manifest.label = label.map(str::to_string);
    let (run_dir, existing_rows) = if let Some(run_dir) = resume {
        let mut existing_manifest = validate_resume(run_dir, &manifest)?;
        if label.is_some() {
            existing_manifest.label = manifest.label.clone();
            write_manifest(run_dir, &existing_manifest).await?;
        }
        (run_dir.to_string(), load_scorecard(run_dir).await?)
    } else {
        let run_dir = create_run_dir().await?;
        write_manifest(&run_dir, &manifest).await?;
        // Leave evidence that the bench started even if it is interrupted before
        // its first retrieval completes.
        write_scorecard(&run_dir, &[]).await?;
        (run_dir, Vec::new())
    };

    let total = cases.len() * strategies.len() * repeat;
    let in_flight = concurrency.min(total).max(1);
    writeln!(
        stdout,
        "live: {} cases x {} {} x {repeat} samples = {total} runs; concurrency {in_flight}",
        cases.len(),
        strategies.len(),
        strategy_noun(strategies.len()),
    )
    .map_err(|error| format!("failed to print live plan: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("failed to flush live plan: {error}"))?;

    let network_slots = Arc::new(Semaphore::new(in_flight));
    let playwright_slots = Arc::new(Semaphore::new(1));
    let mut jobs = JoinSet::new();
    let mut existing = existing_rows
        .into_iter()
        .map(|row| ((row.case_id, row.strategy.clone(), row.repeat_index), row))
        .collect::<BTreeMap<_, _>>();
    let mut ranked = Vec::with_capacity(total);
    let mut order = 0usize;
    for case in &cases {
        for strategy in strategies {
            for repeat_index in 1..=repeat {
                let key = (case.id, strategy.as_str().to_string(), repeat_index);
                if let Some(row) = existing.remove(&key)
                    && !should_retry_image(&row, retry)
                {
                    ranked.push((order, row));
                } else {
                    jobs.spawn(run_live_job(
                        order,
                        repeat_index,
                        case.clone(),
                        *strategy,
                        run_dir.clone(),
                        network_slots.clone(),
                        playwright_slots.clone(),
                    ));
                }
                order += 1;
            }
        }
    }

    if !existing.is_empty() {
        return Err("resume scorecard contains rows outside the configured job set".to_string());
    }
    let mut done = ranked.len();
    if resume.is_some() {
        writeln!(
            stdout,
            "resume: kept {done} completed jobs; running {}",
            total - done
        )
        .map_err(|error| format!("failed to print resume plan: {error}"))?;
    }
    while let Some(joined) = jobs.join_next().await {
        let (job_order, row) =
            joined.map_err(|error| format!("lab image job panicked: {error}"))??;
        done += 1;
        writeln!(stdout, "{}", progress_line(done, total, &row))
            .map_err(|error| format!("failed to print job progress: {error}"))?;
        stdout
            .flush()
            .map_err(|error| format!("failed to flush job progress: {error}"))?;
        ranked.push((job_order, row));
        let rows = ordered_rows(&ranked);
        write_scorecard(&run_dir, &rows).await?;
    }
    let rows = ordered_rows(&ranked);

    let hit = rows
        .iter()
        .filter(|row| row.search_or_download == "hit")
        .count();
    let miss = rows.len() - hit;
    writeln!(stdout, "scorecard: {run_dir}/scorecard.md")
        .map_err(|error| format!("failed to print scorecard path: {error}"))?;
    writeln!(
        stdout,
        "summary: {} cases x {} {} x {repeat} samples = {} runs; hit {hit}, miss {miss}; {}",
        cases.len(),
        strategies.len(),
        strategy_noun(strategies.len()),
        rows.len(),
        aggregate_elapsed(&rows),
    )
    .map_err(|error| format!("failed to print scorecard summary: {error}"))?;
    Ok(())
}

fn should_retry_image(row: &ScorecardRow, retry: Option<RetryPolicy>) -> bool {
    match retry {
        None => false,
        Some(RetryPolicy::Miss) => row.search_or_download == "miss" && row.kind != "timeout",
        Some(RetryPolicy::Timeout) => row.kind == "timeout",
    }
}

async fn load_scorecard(run_dir: &str) -> Result<Vec<ScorecardRow>, String> {
    let path = format!("{run_dir}/scorecard.json");
    let json = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| format!("failed to read resume scorecard `{path}`: {error}"))?;
    serde_json::from_str(&json)
        .map_err(|error| format!("failed to parse resume scorecard `{path}`: {error}"))
}

const fn default_repeat_index() -> usize {
    1
}

async fn run_live_job(
    order: usize,
    repeat_index: usize,
    case: ImageCase,
    strategy: ImageStrategy,
    run_dir: String,
    network_slots: Arc<Semaphore>,
    playwright_slots: Arc<Semaphore>,
) -> Result<(usize, ScorecardRow), String> {
    let queued = Instant::now();
    let network_permit = network_slots
        .acquire_owned()
        .await
        .expect("lab network slot is never closed");
    let playwright_permit = if strategy == ImageStrategy::BingPlaywright {
        Some(
            playwright_slots
                .acquire_owned()
                .await
                .expect("lab Playwright slot is never closed"),
        )
    } else {
        None
    };
    let queue_ms = millis(queued.elapsed());
    let started = Instant::now();
    let outcome = match tokio::time::timeout(
        RETRIEVE_TIMEOUT,
        retrieve_live_image(&case, repeat_index, strategy, &run_dir),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            tracing::warn!(
                case_id = case.id,
                strategy = strategy.as_str(),
                timeout_secs = RETRIEVE_TIMEOUT.as_secs(),
                "lab image job exceeded the deadline"
            );
            RetrievalOutcome::TimedOut
        }
    };
    let elapsed_ms = millis(started.elapsed());
    drop(playwright_permit);
    drop(network_permit);

    Ok((
        order,
        scorecard_row(
            &case,
            repeat_index,
            strategy,
            &outcome,
            elapsed_ms,
            queue_ms,
        ),
    ))
}

async fn retrieve_live_image(
    case: &ImageCase,
    repeat_index: usize,
    strategy: ImageStrategy,
    run_dir: &str,
) -> Result<RetrievalOutcome, String> {
    let reasoning = ReasoningConfig::new("none");
    let input = ImageInput {
        topic_name: &case.topic_name,
        card_title: &case.card_title,
        card_content: &case.card_content,
        image_query: &case.image_query,
        model: "",
        api_key: "",
        api_base: "",
        reasoning: &reasoning,
        pool: &[],
    };

    let outcome = RetrievalOutcome::Retrieved(crate::llm::retrieve_image(strategy, input).await);

    if let RetrievalOutcome::Retrieved(Some(RetrievedImage::Prepared(prepared))) = &outcome {
        let case_dir = format!("{run_dir}/cases/{}", case.id);
        tokio::fs::create_dir_all(&case_dir)
            .await
            .map_err(|error| {
                format!("failed to create image case directory `{case_dir}`: {error}")
            })?;
        let extension = safe_extension(&prepared.extension);
        let image_path = format!(
            "{case_dir}/{}-{repeat_index}.{extension}",
            strategy.as_str()
        );
        tokio::fs::write(&image_path, &prepared.bytes)
            .await
            .map_err(|error| format!("failed to write prepared image `{image_path}`: {error}"))?;
    }

    Ok(outcome)
}

enum RetrievalOutcome {
    Retrieved(Option<RetrievedImage>),
    TimedOut,
}

pub(crate) fn dry_run_plan(
    cases_path: &str,
    strategies: &[ImageStrategy],
    cases: &[ImageCase],
    concurrency: usize,
    repeat: usize,
) -> String {
    let strategy_count = strategies.len();
    let strategy_names = strategies
        .iter()
        .map(|strategy| strategy.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let total = cases.len() * strategy_count * repeat;
    let in_flight = concurrency.min(total).max(1);

    let mut plan = format!(
        "bench: images\nmode: dry-run (no downloads)\ncases: {cases_path}\nstrategies: {strategy_names}\nrepeat: {repeat}\nconcurrency: {in_flight}\nplan:\n"
    );
    for case in cases {
        plan.push_str(&format!(
            "  [{}] {} — {} | `{}`\n",
            case.id, case.topic_name, case.card_title, case.image_query
        ));
    }
    plan.push_str(&format!(
        "{} cases x {} {} x {repeat} samples = {} runs (0 downloads)\n",
        cases.len(),
        strategy_count,
        strategy_noun(strategy_count),
        cases.len() * strategy_count * repeat,
    ));
    plan
}

fn strategy_noun(count: usize) -> &'static str {
    if count == 1 { "strategy" } else { "strategies" }
}

fn progress_line(done: usize, total: usize, row: &ScorecardRow) -> String {
    if row.search_or_download == "hit" {
        format!(
            "[{done}/{total}] {} {} hit {}ms {} bytes",
            row.case_id, row.strategy, row.elapsed_ms, row.bytes
        )
    } else if row.kind == "timeout" {
        format!(
            "[{done}/{total}] {} {} timeout {}ms",
            row.case_id, row.strategy, row.elapsed_ms
        )
    } else {
        format!(
            "[{done}/{total}] {} {} miss {}ms",
            row.case_id, row.strategy, row.elapsed_ms
        )
    }
}

fn scorecard_row(
    case: &ImageCase,
    repeat_index: usize,
    strategy: ImageStrategy,
    outcome: &RetrievalOutcome,
    elapsed_ms: u64,
    queue_ms: u64,
) -> ScorecardRow {
    let (search_or_download, kind, bytes, mime_type, extension) = match outcome {
        RetrievalOutcome::Retrieved(Some(RetrievedImage::Prepared(prepared))) => (
            "hit".to_string(),
            "prepared".to_string(),
            prepared.bytes.len(),
            Some(prepared.mime_type.clone()),
            Some(prepared.extension.clone()),
        ),
        RetrievalOutcome::Retrieved(Some(RetrievedImage::Pool(pool_id))) => {
            ("hit".to_string(), format!("pool:{pool_id}"), 0, None, None)
        }
        RetrievalOutcome::Retrieved(None) => {
            ("miss".to_string(), "none".to_string(), 0, None, None)
        }
        RetrievalOutcome::TimedOut => ("miss".to_string(), "timeout".to_string(), 0, None, None),
    };

    ScorecardRow {
        case_id: case.id,
        repeat_index,
        strategy: strategy.as_str().to_string(),
        search_or_download,
        kind,
        bytes,
        mime_type,
        extension,
        elapsed_ms,
        queue_ms,
        failure_stage: match outcome {
            RetrievalOutcome::Retrieved(None) => Some("production_returned_none".to_string()),
            RetrievalOutcome::TimedOut => Some("retrieval_timeout".to_string()),
            RetrievalOutcome::Retrieved(Some(_)) => None,
        },
        visual: "needs_review".to_string(),
        expected: case.expected.clone(),
    }
}

pub(crate) fn scorecard_markdown(rows: &[ScorecardRow]) -> String {
    let mut markdown = String::from(
        "| case_id | sample | strategy | search_or_download | kind | bytes | mime_type | extension | elapsed_ms | queue_ms | failure_stage | visual | expected |\n\
         |---:|---:|---|---|---|---:|---|---|---:|---:|---|---|---|\n",
    );
    for row in rows {
        markdown.push_str(&format!(
            "| {case_id} | {repeat_index} | {strategy} | {search_or_download} | {kind} | {bytes} | {mime_type} | {extension} | {elapsed_ms} | {queue_ms} | {failure_stage} | {visual} | {expected} |\n",
            case_id = row.case_id,
            repeat_index = row.repeat_index,
            strategy = markdown_cell(&row.strategy),
            search_or_download = markdown_cell(&row.search_or_download),
            kind = markdown_cell(&row.kind),
            bytes = row.bytes,
            mime_type = markdown_cell(row.mime_type.as_deref().unwrap_or("")),
            extension = markdown_cell(row.extension.as_deref().unwrap_or("")),
            elapsed_ms = row.elapsed_ms,
            queue_ms = row.queue_ms,
            failure_stage = markdown_cell(row.failure_stage.as_deref().unwrap_or("")),
            visual = markdown_cell(&row.visual),
            expected = markdown_cell(&row.expected),
        ));
    }
    markdown
}

fn select_cases(
    cases: Vec<ImageCase>,
    selected: &[u64],
    tags: &[String],
) -> Result<Vec<ImageCase>, String> {
    if selected.is_empty() && tags.is_empty() {
        return Ok(cases);
    }
    let filtered = cases
        .into_iter()
        .filter(|case| {
            (selected.is_empty() || selected.contains(&case.id))
                && (tags.is_empty() || case.tags.iter().any(|tag| tags.contains(tag)))
        })
        .collect::<Vec<_>>();
    let missing = selected
        .iter()
        .filter(|id| !filtered.iter().any(|case| case.id == **id))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "image case pack has no selected case id(s): {missing:?}"
        ));
    }
    if filtered.is_empty() {
        return Err(format!("no image cases match selected tag(s): {tags:?}"));
    }
    Ok(filtered)
}

fn aggregate_elapsed(rows: &[ScorecardRow]) -> String {
    let mut elapsed = rows.iter().map(|row| row.elapsed_ms).collect::<Vec<_>>();
    elapsed.sort_unstable();
    if elapsed.is_empty() {
        return "latency n/a".to_string();
    }
    let median = elapsed[(elapsed.len() - 1) / 2];
    let p95 = elapsed[((elapsed.len() * 95).div_ceil(100)).saturating_sub(1)];
    format!("latency median {median}ms, p95 {p95}ms")
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
        .replace('\r', "")
}

fn ordered_rows(ranked: &[(usize, ScorecardRow)]) -> Vec<ScorecardRow> {
    let mut ordered = ranked.to_vec();
    ordered.sort_by_key(|(job_order, _)| *job_order);
    ordered.into_iter().map(|(_, row)| row).collect()
}

async fn write_scorecard(run_dir: &str, rows: &[ScorecardRow]) -> Result<(), String> {
    let markdown_path = format!("{run_dir}/scorecard.md");
    write_atomically(&markdown_path, scorecard_markdown(rows)).await?;

    let json_path = format!("{run_dir}/scorecard.json");
    let json = serde_json::to_string_pretty(rows)
        .map_err(|error| format!("failed to serialize scorecard JSON: {error}"))?;
    write_atomically(&json_path, json).await
}

async fn write_atomically(path: &str, contents: String) -> Result<(), String> {
    let temporary_path = format!("{path}.tmp");
    tokio::fs::write(&temporary_path, contents)
        .await
        .map_err(|error| {
            format!("failed to write temporary scorecard `{temporary_path}`: {error}")
        })?;
    tokio::fs::rename(&temporary_path, path)
        .await
        .map_err(|error| format!("failed to checkpoint scorecard `{path}`: {error}"))
}

async fn create_run_dir() -> Result<String, String> {
    const ROOT: &str = "lab/runs";
    tokio::fs::create_dir_all(ROOT)
        .await
        .map_err(|error| format!("failed to create `{ROOT}`: {error}"))?;

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    for attempt in 0_u64.. {
        let run_dir = run_dir_candidate(&stamp, attempt);
        match tokio::fs::create_dir(&run_dir).await {
            Ok(()) => return Ok(run_dir),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("failed to create `{run_dir}`: {error}")),
        }
    }
    unreachable!("unbounded run-directory attempt counter cannot be exhausted")
}

fn run_dir_candidate(stamp: &str, attempt: u64) -> String {
    if attempt == 0 {
        format!("lab/runs/{stamp}-images")
    } else {
        format!("lab/runs/{stamp}-images-{attempt}")
    }
}

fn safe_extension(extension: &str) -> &str {
    if !extension.is_empty() && extension.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        extension
    } else {
        "bin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::run_with;

    #[tokio::test]
    async fn dry_run_returns_zero_prints_all_gold_queries_and_writes_nothing() {
        let runs_before = run_dir_snapshot();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "images".to_string(),
                "--dry-run".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 0);
        assert!(
            stderr.is_empty(),
            "dry-run stderr must be empty: {stderr:?}"
        );
        let output = String::from_utf8(stdout).expect("stdout must be UTF-8");
        for query in [
            "diagram prepositions of movement from to into toward",
            "diagram of in on at prepositions of place",
            "diagram of adjective order before nouns English grammar",
            "rust clippy pedantic lints screenshot",
            "helix editor modal text editor screenshot",
        ] {
            assert!(output.contains(query), "dry-run must print `{query}`");
        }
        assert!(
            output.contains("concurrency: 5"),
            "dry-run must print default concurrency: {output}"
        );
        assert_eq!(
            run_dir_snapshot(),
            runs_before,
            "dry-run must not create files under lab/runs/"
        );
    }

    #[tokio::test]
    async fn dry_run_honors_concurrency_flag() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "images".to_string(),
                "--dry-run".to_string(),
                "--concurrency".to_string(),
                "2".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 0);
        assert!(stderr.is_empty(), "stderr: {stderr:?}");
        let output = String::from_utf8(stdout).expect("stdout must be UTF-8");
        assert!(
            output.contains("concurrency: 2"),
            "dry-run must print requested concurrency: {output}"
        );
    }

    #[test]
    fn progress_line_distinguishes_hit_miss_and_timeout() {
        let hit = ScorecardRow {
            case_id: 270,
            repeat_index: 1,
            strategy: "bing_html".to_string(),
            search_or_download: "hit".to_string(),
            kind: "prepared".to_string(),
            bytes: 1234,
            mime_type: Some("image/png".to_string()),
            extension: Some("png".to_string()),
            elapsed_ms: 42,
            queue_ms: 0,
            failure_stage: None,
            visual: "needs_review".to_string(),
            expected: "movement diagram".to_string(),
        };
        let miss = ScorecardRow {
            case_id: 286,
            repeat_index: 1,
            strategy: "ddgs_text_og".to_string(),
            search_or_download: "miss".to_string(),
            kind: "none".to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 99,
            queue_ms: 0,
            failure_stage: Some("production_returned_none".to_string()),
            visual: "needs_review".to_string(),
            expected: "place diagram".to_string(),
        };
        let timeout = ScorecardRow {
            case_id: 8,
            repeat_index: 1,
            strategy: "bing_html".to_string(),
            search_or_download: "miss".to_string(),
            kind: "timeout".to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 90_000,
            queue_ms: 12,
            failure_stage: Some("retrieval_timeout".to_string()),
            visual: "needs_review".to_string(),
            expected: "helix".to_string(),
        };

        assert_eq!(
            progress_line(1, 5, &hit),
            "[1/5] 270 bing_html hit 42ms 1234 bytes"
        );
        assert_eq!(
            progress_line(2, 5, &miss),
            "[2/5] 286 ddgs_text_og miss 99ms"
        );
        assert_eq!(
            progress_line(3, 5, &timeout),
            "[3/5] 8 bing_html timeout 90000ms"
        );
    }

    fn run_dir_snapshot() -> Vec<std::path::PathBuf> {
        let root = std::path::Path::new("lab/runs");
        let mut paths = Vec::new();
        if !root.exists() {
            return paths;
        }

        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).expect("runs dir readable").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
    }

    #[test]
    fn scorecard_markdown_keeps_mechanical_and_visual_columns_separate() {
        let rows = vec![
            ScorecardRow {
                case_id: 270,
                repeat_index: 1,
                strategy: "bing_html".to_string(),
                search_or_download: "hit".to_string(),
                kind: "prepared".to_string(),
                bytes: 1234,
                mime_type: Some("image/png".to_string()),
                extension: Some("png".to_string()),
                elapsed_ms: 42,
                queue_ms: 0,
                failure_stage: None,
                visual: "needs_review".to_string(),
                expected: "movement diagram".to_string(),
            },
            ScorecardRow {
                case_id: 286,
                repeat_index: 1,
                strategy: "ddgs_text_og".to_string(),
                search_or_download: "miss".to_string(),
                kind: "none".to_string(),
                bytes: 0,
                mime_type: None,
                extension: None,
                elapsed_ms: 99,
                queue_ms: 0,
                failure_stage: Some("production_returned_none".to_string()),
                visual: "needs_review".to_string(),
                expected: "place diagram".to_string(),
            },
        ];

        let markdown = scorecard_markdown(&rows);

        assert!(markdown.contains("| search_or_download |"));
        assert!(markdown.contains("| visual |"));
        let hit_line = markdown
            .lines()
            .find(|line| line.contains("| 270 | 1 | bing_html | hit | prepared |"))
            .expect("hit row must be present");
        let miss_line = markdown
            .lines()
            .find(|line| line.contains("| 286 | 1 | ddgs_text_og | miss | none |"))
            .expect("miss row must be present");
        assert!(hit_line.contains("needs_review"));
        assert!(miss_line.contains("needs_review"));
        assert!(!hit_line.contains("miss"));
        assert!(!miss_line.contains("hit"));
    }

    #[test]
    fn scorecard_markdown_escapes_every_text_cell() {
        let markdown = scorecard_markdown(&[ScorecardRow {
            case_id: 7,
            repeat_index: 1,
            strategy: "bing|html\\stable\nnext".to_string(),
            search_or_download: "hit|maybe".to_string(),
            kind: "prepared\r\nwith|note".to_string(),
            bytes: 1,
            mime_type: Some("image|png\\x".to_string()),
            extension: Some("p|ng\n".to_string()),
            elapsed_ms: 2,
            queue_ms: 3,
            failure_stage: None,
            visual: "needs|review\nnow".to_string(),
            expected: "a\\b|c\r\nd".to_string(),
        }]);

        assert!(markdown.contains("bing\\|html\\\\stable<br>next"));
        assert!(markdown.contains("hit\\|maybe"));
        assert!(markdown.contains("prepared<br>with\\|note"));
        assert!(markdown.contains("image\\|png\\\\x"));
        assert!(markdown.contains("p\\|ng<br>"));
        assert!(markdown.contains("needs\\|review<br>now"));
        assert!(markdown.contains("a\\\\b\\|c<br>d"));
        assert_eq!(markdown.lines().count(), 3, "escaped cells stay on one row");
    }

    #[test]
    fn ordered_rows_keeps_partial_scorecards_in_case_pack_order() {
        let row = |case_id| ScorecardRow {
            case_id,
            repeat_index: 1,
            strategy: "bing_html".to_string(),
            search_or_download: "miss".to_string(),
            kind: "none".to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 1,
            queue_ms: 0,
            failure_stage: None,
            visual: "needs_review".to_string(),
            expected: String::new(),
        };
        let rows = ordered_rows(&[(2, row(30)), (0, row(10)), (1, row(20))]);

        assert_eq!(
            rows.iter().map(|row| row.case_id).collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    #[test]
    fn run_dir_candidates_are_distinct_after_a_collision() {
        let base = run_dir_candidate("20260822T000000.000Z", 0);
        let retry = run_dir_candidate("20260822T000000.000Z", 1);

        assert_eq!(base, "lab/runs/20260822T000000.000Z-images");
        assert_eq!(retry, "lab/runs/20260822T000000.000Z-images-1");
        assert_ne!(base, retry);
    }

    #[test]
    fn retry_policies_select_only_requested_failures() {
        let row = |kind: &str, status: &str| ScorecardRow {
            case_id: 1,
            repeat_index: 1,
            strategy: "bing_html".to_string(),
            search_or_download: status.to_string(),
            kind: kind.to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 1,
            queue_ms: 0,
            failure_stage: None,
            visual: "needs_review".to_string(),
            expected: String::new(),
        };
        assert!(should_retry_image(
            &row("none", "miss"),
            Some(RetryPolicy::Miss)
        ));
        assert!(!should_retry_image(
            &row("none", "miss"),
            Some(RetryPolicy::Timeout)
        ));
        assert!(should_retry_image(
            &row("timeout", "miss"),
            Some(RetryPolicy::Timeout)
        ));
        assert!(!should_retry_image(
            &row("timeout", "miss"),
            Some(RetryPolicy::Miss)
        ));
        assert!(!should_retry_image(
            &row("prepared", "hit"),
            Some(RetryPolicy::Miss)
        ));
    }

    #[tokio::test]
    async fn resume_keeps_completed_jobs_without_network_calls() {
        let run_dir = temporary_run_root("resume-complete");
        tokio::fs::create_dir_all(&run_dir)
            .await
            .expect("resume directory is created");
        let compatibility =
            BTreeMap::from([("strategies".to_string(), serde_json::json!(["bing_html"]))]);
        let execution = BTreeMap::from([
            ("concurrency".to_string(), serde_json::json!(1)),
            ("repeat".to_string(), serde_json::json!(1)),
            ("case_ids".to_string(), serde_json::json!([270])),
            ("tags".to_string(), serde_json::json!([])),
            (
                "timeout_seconds".to_string(),
                serde_json::json!(RETRIEVE_TIMEOUT.as_secs()),
            ),
        ]);
        let manifest = RunManifest::new(
            "images",
            crate::lab::cases::DEFAULT_GOLD_CASES_PATH,
            None,
            None,
            compatibility,
            execution,
        )
        .expect("manifest builds");
        write_manifest(run_dir.to_str().expect("UTF-8 path"), &manifest)
            .await
            .expect("manifest writes");
        let row = ScorecardRow {
            case_id: 270,
            repeat_index: 1,
            strategy: "bing_html".to_string(),
            search_or_download: "hit".to_string(),
            kind: "prepared".to_string(),
            bytes: 10,
            mime_type: Some("image/png".to_string()),
            extension: Some("png".to_string()),
            elapsed_ms: 20,
            queue_ms: 1,
            failure_stage: None,
            visual: "needs_review".to_string(),
            expected: "diagram".to_string(),
        };
        tokio::fs::write(
            run_dir.join("scorecard.json"),
            serde_json::to_string(&[row]).expect("scorecard serializes"),
        )
        .await
        .expect("scorecard writes");
        let mut stdout = Vec::new();
        let config = ImageRunConfig {
            cases_path: crate::lab::cases::DEFAULT_GOLD_CASES_PATH,
            strategies: &[ImageStrategy::BingHtml],
            dry_run: false,
            concurrency: 1,
            case_ids: &[270],
            tags: &[],
            repeat: 1,
            label: None,
            resume: Some(run_dir.to_str().expect("UTF-8 path")),
            retry: None,
        };

        run_images(&config, &mut stdout)
            .await
            .expect("complete resume succeeds without retrieval");
        let output = String::from_utf8(stdout).expect("UTF-8 output");
        assert!(output.contains("resume: kept 1 completed jobs; running 0"));
        tokio::fs::remove_dir_all(run_dir)
            .await
            .expect("resume directory removed");
    }

    fn temporary_run_root(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "denpie-images-{label}-{}-{nonce}",
            std::process::id()
        ))
    }
}
