//! Live image bake-off runner. The dry-run path is CI-safe; the live path
//! deliberately calls production `retrieve_image` and therefore needs network.

use std::io::{ErrorKind, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::domain::grounding::ImageStrategy;
use crate::lab::cases::{ImageCase, load_image_cases};
use crate::llm::{ImageInput, ReasoningConfig, RetrievedImage};

pub(crate) const DEFAULT_CONCURRENCY: usize = 5;
const RETRIEVE_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ScorecardRow {
    pub(crate) case_id: u64,
    pub(crate) strategy: String,
    pub(crate) search_or_download: String,
    pub(crate) kind: String,
    pub(crate) bytes: usize,
    pub(crate) mime_type: Option<String>,
    pub(crate) extension: Option<String>,
    pub(crate) elapsed_ms: u64,
    pub(crate) visual: String,
    pub(crate) expected: String,
}

pub(crate) async fn run_images(
    cases_path: &str,
    strategies: &[ImageStrategy],
    dry_run: bool,
    concurrency: usize,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let cases = load_image_cases(cases_path)?;

    if dry_run {
        write!(
            stdout,
            "{}",
            dry_run_plan(cases_path, strategies, &cases, concurrency)
        )
        .map_err(|error| format!("failed to print dry-run plan: {error}"))?;
        return Ok(());
    }

    let run_dir = create_run_dir().await?;
    // Leave evidence that the bench started even if it is interrupted before
    // its first retrieval completes.
    write_scorecard(&run_dir, &[]).await?;

    let total = cases.len() * strategies.len();
    let in_flight = concurrency.min(total).max(1);
    writeln!(
        stdout,
        "live: {} cases x {} {} = {total} runs; concurrency {in_flight}",
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
    let mut order = 0usize;
    for case in &cases {
        for strategy in strategies {
            jobs.spawn(run_live_job(
                order,
                case.clone(),
                *strategy,
                run_dir.clone(),
                network_slots.clone(),
                playwright_slots.clone(),
            ));
            order += 1;
        }
    }

    let mut ranked = Vec::with_capacity(total);
    let mut done = 0usize;
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
        "summary: {} cases x {} {} = {} runs; hit {hit}, miss {miss}",
        cases.len(),
        strategies.len(),
        strategy_noun(strategies.len()),
        rows.len(),
    )
    .map_err(|error| format!("failed to print scorecard summary: {error}"))?;
    Ok(())
}

async fn run_live_job(
    order: usize,
    case: ImageCase,
    strategy: ImageStrategy,
    run_dir: String,
    network_slots: Arc<Semaphore>,
    playwright_slots: Arc<Semaphore>,
) -> Result<(usize, ScorecardRow), String> {
    let started = Instant::now();
    let outcome = match tokio::time::timeout(
        RETRIEVE_TIMEOUT,
        retrieve_live_image(&case, strategy, &run_dir, network_slots, playwright_slots),
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
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    Ok((order, scorecard_row(&case, strategy, &outcome, elapsed_ms)))
}

async fn retrieve_live_image(
    case: &ImageCase,
    strategy: ImageStrategy,
    run_dir: &str,
    network_slots: Arc<Semaphore>,
    playwright_slots: Arc<Semaphore>,
) -> Result<RetrievalOutcome, String> {
    let _playwright_permit = if strategy == ImageStrategy::BingPlaywright {
        Some(
            playwright_slots
                .acquire_owned()
                .await
                .expect("lab Playwright slot is never closed"),
        )
    } else {
        None
    };
    let _network_permit = network_slots
        .acquire_owned()
        .await
        .expect("lab network slot is never closed");

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
        let image_path = format!("{case_dir}/{}.{extension}", strategy.as_str());
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
) -> String {
    let strategy_count = strategies.len();
    let strategy_names = strategies
        .iter()
        .map(|strategy| strategy.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let total = cases.len() * strategy_count;
    let in_flight = concurrency.min(total).max(1);

    let mut plan = format!(
        "bench: images\nmode: dry-run (no downloads)\ncases: {cases_path}\nstrategies: {strategy_names}\nconcurrency: {in_flight}\nplan:\n"
    );
    for case in cases {
        plan.push_str(&format!(
            "  [{}] {} — {} | `{}`\n",
            case.id, case.topic_name, case.card_title, case.image_query
        ));
    }
    plan.push_str(&format!(
        "{} cases x {} {} = {} runs (0 downloads)\n",
        cases.len(),
        strategy_count,
        strategy_noun(strategy_count),
        cases.len() * strategy_count,
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
    strategy: ImageStrategy,
    outcome: &RetrievalOutcome,
    elapsed_ms: u64,
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
        strategy: strategy.as_str().to_string(),
        search_or_download,
        kind,
        bytes,
        mime_type,
        extension,
        elapsed_ms,
        visual: "needs_review".to_string(),
        expected: case.expected.clone(),
    }
}

pub(crate) fn scorecard_markdown(rows: &[ScorecardRow]) -> String {
    let mut markdown = String::from(
        "| case_id | strategy | search_or_download | kind | bytes | mime_type | extension | elapsed_ms | visual | expected |\n\
         |---:|---|---|---|---:|---|---|---:|---|---|\n",
    );
    for row in rows {
        markdown.push_str(&format!(
            "| {case_id} | {strategy} | {search_or_download} | {kind} | {bytes} | {mime_type} | {extension} | {elapsed_ms} | {visual} | {expected} |\n",
            case_id = row.case_id,
            strategy = markdown_cell(&row.strategy),
            search_or_download = markdown_cell(&row.search_or_download),
            kind = markdown_cell(&row.kind),
            bytes = row.bytes,
            mime_type = markdown_cell(row.mime_type.as_deref().unwrap_or("")),
            extension = markdown_cell(row.extension.as_deref().unwrap_or("")),
            elapsed_ms = row.elapsed_ms,
            visual = markdown_cell(&row.visual),
            expected = markdown_cell(&row.expected),
        ));
    }
    markdown
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
            strategy: "bing_html".to_string(),
            search_or_download: "hit".to_string(),
            kind: "prepared".to_string(),
            bytes: 1234,
            mime_type: Some("image/png".to_string()),
            extension: Some("png".to_string()),
            elapsed_ms: 42,
            visual: "needs_review".to_string(),
            expected: "movement diagram".to_string(),
        };
        let miss = ScorecardRow {
            case_id: 286,
            strategy: "ddgs_text_og".to_string(),
            search_or_download: "miss".to_string(),
            kind: "none".to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 99,
            visual: "needs_review".to_string(),
            expected: "place diagram".to_string(),
        };
        let timeout = ScorecardRow {
            case_id: 8,
            strategy: "bing_html".to_string(),
            search_or_download: "miss".to_string(),
            kind: "timeout".to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 90_000,
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
                strategy: "bing_html".to_string(),
                search_or_download: "hit".to_string(),
                kind: "prepared".to_string(),
                bytes: 1234,
                mime_type: Some("image/png".to_string()),
                extension: Some("png".to_string()),
                elapsed_ms: 42,
                visual: "needs_review".to_string(),
                expected: "movement diagram".to_string(),
            },
            ScorecardRow {
                case_id: 286,
                strategy: "ddgs_text_og".to_string(),
                search_or_download: "miss".to_string(),
                kind: "none".to_string(),
                bytes: 0,
                mime_type: None,
                extension: None,
                elapsed_ms: 99,
                visual: "needs_review".to_string(),
                expected: "place diagram".to_string(),
            },
        ];

        let markdown = scorecard_markdown(&rows);

        assert!(markdown.contains("| search_or_download |"));
        assert!(markdown.contains("| visual |"));
        let hit_line = markdown
            .lines()
            .find(|line| line.contains("| 270 | bing_html | hit | prepared |"))
            .expect("hit row must be present");
        let miss_line = markdown
            .lines()
            .find(|line| line.contains("| 286 | ddgs_text_og | miss | none |"))
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
            strategy: "bing|html\\stable\nnext".to_string(),
            search_or_download: "hit|maybe".to_string(),
            kind: "prepared\r\nwith|note".to_string(),
            bytes: 1,
            mime_type: Some("image|png\\x".to_string()),
            extension: Some("p|ng\n".to_string()),
            elapsed_ms: 2,
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
            strategy: "bing_html".to_string(),
            search_or_download: "miss".to_string(),
            kind: "none".to_string(),
            bytes: 0,
            mime_type: None,
            extension: None,
            elapsed_ms: 1,
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
}
