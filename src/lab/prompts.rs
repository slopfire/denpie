//! Prompt bake-off runner. The dry-run path is CI-safe; the live path calls
//! production `generate_card` for one-shot cases and therefore needs a real
//! `DENPIE_LAB_LLM_API_KEY`. Array cases are assembled but not generated.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::context::CardContext;
use crate::lab::cases::{PromptCase, load_prompt_cases};
use crate::llm::cards::{GeneratedCard, assemble_array_prompt, assemble_one_shot_prompt};
use crate::llm::{CompressionLevel, DEFAULT_PROMPT_TEMPLATE, ReasoningConfig};

pub(crate) const API_KEY_ENV: &str = "DENPIE_LAB_LLM_API_KEY";
pub(crate) const MODEL_ENV: &str = "DENPIE_LAB_LLM_MODEL";
pub(crate) const BASE_URL_ENV: &str = "DENPIE_LAB_LLM_BASE_URL";
pub(crate) const DEFAULT_MODEL: &str = "google/gemini-3.1-flash";
pub(crate) const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const GENERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);
const RUNS_DIR: &str = "lab/runs";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PromptScorecardRow {
    pub(crate) case_id: String,
    pub(crate) topic: String,
    pub(crate) mode: String,
    pub(crate) compression: String,
    pub(crate) batch_count: usize,
    pub(crate) assembled: bool,
    pub(crate) generated: String,
    pub(crate) kind: String,
    pub(crate) title_words: Option<usize>,
    pub(crate) full_content_words: Option<usize>,
    pub(crate) compressed_content_words: Option<usize>,
    pub(crate) use_image: bool,
    pub(crate) prompt_tokens: i64,
    pub(crate) completion_tokens: i64,
    pub(crate) total_tokens: i64,
    pub(crate) elapsed_ms: u64,
    pub(crate) error: Option<String>,
    pub(crate) visual: String,
    pub(crate) expected: String,
}

#[derive(Debug)]
struct LivePromptSettings {
    model: String,
    api_key: String,
    api_base: String,
}

impl LivePromptSettings {
    fn from_env() -> Result<Self, String> {
        let api_key = std::env::var(API_KEY_ENV).unwrap_or_default();
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err(format!(
                "live prompt runs require `{API_KEY_ENV}`; export a real API key and retry"
            ));
        }

        Ok(Self {
            model: env_or(MODEL_ENV, DEFAULT_MODEL),
            api_key: api_key.to_string(),
            api_base: env_or(BASE_URL_ENV, DEFAULT_BASE_URL),
        })
    }
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub(crate) async fn run_prompts(
    cases_path: &str,
    dry_run: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    if dry_run {
        let cases = load_prompt_cases(cases_path)?;
        write!(stdout, "{}", dry_run_plan(cases_path, &cases))
            .map_err(|error| format!("failed to print prompt dry-run plan: {error}"))?;
        return Ok(());
    }

    let settings = LivePromptSettings::from_env()?;
    let cases = load_prompt_cases(cases_path)?;
    let run_dir = create_run_dir().await?;
    let cases_dir = format!("{run_dir}/cases");
    tokio::fs::create_dir_all(&cases_dir)
        .await
        .map_err(|error| format!("failed to create `{cases_dir}`: {error}"))?;

    writeln!(
        stdout,
        "live: {} prompt cases; model {}; api_base {}",
        cases.len(),
        settings.model,
        settings.api_base
    )
    .map_err(|error| format!("failed to print live prompt plan: {error}"))?;
    stdout
        .flush()
        .map_err(|error| format!("failed to flush live prompt plan: {error}"))?;

    let reasoning = ReasoningConfig::new("none");
    let mut rows = Vec::with_capacity(cases.len());
    let total = cases.len();
    for (index, case) in cases.iter().enumerate() {
        let row = run_live_case(case, &run_dir, &settings, &reasoning).await?;
        let progress = progress_line(index + 1, total, &row);
        rows.push(row);
        write_scorecard(&run_dir, &rows).await?;
        writeln!(stdout, "{progress}")
            .map_err(|error| format!("failed to print prompt job progress: {error}"))?;
        stdout
            .flush()
            .map_err(|error| format!("failed to flush prompt job progress: {error}"))?;
    }

    let hit = rows.iter().filter(|row| row.generated == "hit").count();
    let assembled_only = rows
        .iter()
        .filter(|row| row.kind == "assembled_only")
        .count();
    let miss = rows.len() - hit - assembled_only;
    writeln!(stdout, "scorecard: {run_dir}/scorecard.md")
        .map_err(|error| format!("failed to print prompt scorecard path: {error}"))?;
    writeln!(
        stdout,
        "summary: {} cases; hit {hit}, assembled_only {assembled_only}, miss {miss}",
        cases.len(),
    )
    .map_err(|error| format!("failed to print prompt scorecard summary: {error}"))?;
    Ok(())
}

async fn run_live_case(
    case: &PromptCase,
    run_dir: &str,
    settings: &LivePromptSettings,
    reasoning: &ReasoningConfig,
) -> Result<PromptScorecardRow, String> {
    let rendered = render_case_prompt(case);
    let prompt = assemble_case_prompt(case);
    let prompt_path = format!("{run_dir}/cases/{}.prompt.txt", case.id);
    tokio::fs::write(&prompt_path, &prompt)
        .await
        .map_err(|error| format!("failed to write `{prompt_path}`: {error}"))?;

    let compression = CompressionLevel::from_setting(&case.compression);
    let started = Instant::now();
    let outcome = if case.mode == "array" {
        PromptOutcome::AssembledOnly
    } else {
        match tokio::time::timeout(
            GENERATION_TIMEOUT,
            crate::llm::generate_card(
                &rendered,
                compression,
                &settings.model,
                &settings.api_key,
                &settings.api_base,
                reasoning,
            ),
        )
        .await
        {
            Ok(Ok(card)) => PromptOutcome::Generated(card),
            Ok(Err(error)) => {
                let error = redact_error(&error, &settings.api_key);
                tracing::warn!(
                    case_id = %case.id,
                    error = %error,
                    "lab prompt generation missed"
                );
                PromptOutcome::Failed(error)
            }
            Err(_) => {
                let error = format!(
                    "generation exceeded {}-second deadline",
                    GENERATION_TIMEOUT.as_secs()
                );
                tracing::warn!(
                    case_id = %case.id,
                    timeout_secs = GENERATION_TIMEOUT.as_secs(),
                    "lab prompt generation exceeded the job deadline"
                );
                PromptOutcome::TimedOut(error)
            }
        }
    };
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    if let PromptOutcome::Generated(card) = &outcome {
        let card_path = format!("{run_dir}/cases/{}.card.json", case.id);
        let record = PromptCardRecord::from_card(card);
        let json = serde_json::to_string_pretty(&record).map_err(|error| {
            format!("failed to serialize generated card `{card_path}`: {error}")
        })?;
        tokio::fs::write(&card_path, json)
            .await
            .map_err(|error| format!("failed to write `{card_path}`: {error}"))?;
    }

    Ok(scorecard_row(case, &outcome, elapsed_ms))
}

enum PromptOutcome {
    Generated(GeneratedCard),
    AssembledOnly,
    Failed(String),
    TimedOut(String),
}

fn redact_error(error: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        error.to_string()
    } else {
        error.replace(api_key, "[redacted]")
    }
}

#[derive(Serialize)]
struct PromptCardRecord<'a> {
    title: &'a str,
    full_content: &'a str,
    compressed_content: &'a str,
    use_image: bool,
    image_query: &'a str,
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

impl<'a> PromptCardRecord<'a> {
    fn from_card(card: &'a GeneratedCard) -> Self {
        Self {
            title: &card.title,
            full_content: &card.full_content,
            compressed_content: &card.compressed_content,
            use_image: card.use_image,
            image_query: &card.image_query,
            prompt_tokens: card.usage.prompt_tokens,
            completion_tokens: card.usage.completion_tokens,
            total_tokens: card.usage.total_tokens,
        }
    }
}

pub(crate) fn render_case_prompt(case: &PromptCase) -> String {
    let template = case
        .template
        .as_deref()
        .map(str::trim)
        .filter(|template| !template.is_empty())
        .unwrap_or(DEFAULT_PROMPT_TEMPLATE);
    let context = CardContext::from_parts(
        case.existing_titles.clone(),
        case.dismissed_titles.clone(),
        case.known_items.clone(),
        case.difficult_items.clone(),
        case.uninterested_items.clone(),
    );
    crate::context::render_generation_prompt(&case.topic, template, &context)
}

pub(crate) fn assemble_case_prompt(case: &PromptCase) -> String {
    let rendered = render_case_prompt(case);
    let compression = CompressionLevel::from_setting(&case.compression);
    if case.mode == "array" {
        assemble_array_prompt(&rendered, case.batch_count)
    } else {
        assemble_one_shot_prompt(&rendered, compression)
    }
}

pub(crate) fn dry_run_plan(cases_path: &str, cases: &[PromptCase]) -> String {
    let mut plan =
        format!("bench: prompts\nmode: dry-run (no LLM calls)\ncases: {cases_path}\nplan:\n");
    for case in cases {
        let prompt = assemble_case_prompt(case);
        plan.push_str(&format!(
            "\n[{id}]\ntopic: {topic}\nmode: {mode}\ncompression: {compression}\nbatch_count: {batch_count}\nprompt_length: {prompt_length}\nprompt:\n{prompt}\n",
            id = case.id,
            topic = case.topic,
            mode = case.mode,
            compression = case.compression,
            batch_count = case.batch_count,
            prompt_length = prompt.len(),
        ));
    }
    plan.push_str(&format!("{} cases (0 LLM calls)\n", cases.len()));
    plan
}

fn scorecard_row(
    case: &PromptCase,
    outcome: &PromptOutcome,
    elapsed_ms: u64,
) -> PromptScorecardRow {
    let (
        generated,
        kind,
        title_words,
        full_content_words,
        compressed_content_words,
        use_image,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        error,
    ) = match outcome {
        PromptOutcome::Generated(card) => (
            "hit".to_string(),
            "generated".to_string(),
            Some(card.title.split_whitespace().count()),
            Some(word_count(&card.full_content)),
            Some(word_count(&card.compressed_content)),
            card.use_image,
            card.usage.prompt_tokens,
            card.usage.completion_tokens,
            card.usage.total_tokens,
            None,
        ),
        PromptOutcome::AssembledOnly => (
            "miss".to_string(),
            "assembled_only".to_string(),
            None,
            None,
            None,
            false,
            0,
            0,
            0,
            None,
        ),
        PromptOutcome::Failed(error) => (
            "miss".to_string(),
            "error".to_string(),
            None,
            None,
            None,
            false,
            0,
            0,
            0,
            Some(error.clone()),
        ),
        PromptOutcome::TimedOut(error) => (
            "miss".to_string(),
            "timeout".to_string(),
            None,
            None,
            None,
            false,
            0,
            0,
            0,
            Some(error.clone()),
        ),
    };

    PromptScorecardRow {
        case_id: case.id.clone(),
        topic: case.topic.clone(),
        mode: case.mode.clone(),
        compression: case.compression.clone(),
        batch_count: case.batch_count,
        assembled: true,
        generated,
        kind,
        title_words,
        full_content_words,
        compressed_content_words,
        use_image,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        elapsed_ms,
        error,
        visual: "needs_review".to_string(),
        expected: case.expected.clone(),
    }
}

fn word_count(value: &str) -> usize {
    value.split_whitespace().count()
}

fn progress_line(done: usize, total: usize, row: &PromptScorecardRow) -> String {
    format!(
        "[{done}/{total}] {} {} {} {}ms",
        row.case_id, row.mode, row.kind, row.elapsed_ms
    )
}

pub(crate) fn scorecard_markdown(rows: &[PromptScorecardRow]) -> String {
    let mut markdown = String::from(
        "| case_id | topic | mode | compression | batch_count | assembled | generated | kind | title_words | full_content_words | compressed_content_words | use_image | prompt_tokens | completion_tokens | total_tokens | elapsed_ms | error | visual | expected |\n\
         |---|:---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---|---|\n",
    );
    for row in rows {
        markdown.push_str(&format!(
            "| {case_id} | {topic} | {mode} | {compression} | {batch_count} | {assembled} | {generated} | {kind} | {title_words} | {full_content_words} | {compressed_content_words} | {use_image} | {prompt_tokens} | {completion_tokens} | {total_tokens} | {elapsed_ms} | {error} | {visual} | {expected} |\n",
            case_id = markdown_cell(&row.case_id),
            topic = markdown_cell(&row.topic),
            mode = row.mode,
            compression = row.compression,
            batch_count = row.batch_count,
            assembled = row.assembled,
            generated = row.generated,
            kind = row.kind,
            title_words = row
                .title_words
                .map(|count| count.to_string())
                .unwrap_or_default(),
            full_content_words = row
                .full_content_words
                .map(|count| count.to_string())
                .unwrap_or_default(),
            compressed_content_words = row
                .compressed_content_words
                .map(|count| count.to_string())
                .unwrap_or_default(),
            use_image = row.use_image,
            prompt_tokens = row.prompt_tokens,
            completion_tokens = row.completion_tokens,
            total_tokens = row.total_tokens,
            elapsed_ms = row.elapsed_ms,
            error = row.error.as_deref().map(markdown_cell).unwrap_or_default(),
            visual = row.visual,
            expected = markdown_cell(&row.expected),
        ));
    }
    markdown
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

async fn write_scorecard(run_dir: &str, rows: &[PromptScorecardRow]) -> Result<(), String> {
    let markdown_path = format!("{run_dir}/scorecard.md");
    write_atomically(&markdown_path, scorecard_markdown(rows)).await?;

    let json_path = format!("{run_dir}/scorecard.json");
    let json = serde_json::to_string_pretty(rows)
        .map_err(|error| format!("failed to serialize prompt scorecard JSON: {error}"))?;
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
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ").to_string();
    let path = create_unique_run_dir(Path::new(RUNS_DIR), &stamp).await?;
    Ok(path.to_string_lossy().into_owned())
}

async fn create_unique_run_dir(root: &Path, stamp: &str) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(root).await.map_err(|error| {
        format!(
            "failed to create prompt run root `{}`: {error}",
            root.display()
        )
    })?;

    for suffix in 0..=999_u16 {
        let path = root.join(run_dir_name(stamp, suffix));
        match tokio::fs::create_dir(&path).await {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create prompt run directory `{}`: {error}",
                    path.display()
                ));
            }
        }
    }

    Err(format!(
        "could not allocate a unique prompt run directory for `{stamp}`"
    ))
}

fn run_dir_name(stamp: &str, suffix: u16) -> String {
    if suffix == 0 {
        format!("{stamp}-prompts")
    } else {
        format!("{stamp}-prompts-{suffix:03}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::run_with;

    #[test]
    fn rendered_and_assembled_prompts_have_the_expected_relationship() {
        let case = PromptCase {
            id: "one-shot".to_string(),
            topic: "Rust".to_string(),
            template: None,
            compression: "strong".to_string(),
            mode: "one_shot".to_string(),
            batch_count: 5,
            existing_titles: Vec::new(),
            dismissed_titles: Vec::new(),
            known_items: Vec::new(),
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
            expected: "one-shot prompt".to_string(),
        };

        let rendered = render_case_prompt(&case);
        let assembled = assemble_case_prompt(&case);

        assert!(rendered.contains("Rust"));
        assert!(!rendered.contains("use_image"));
        assert!(assembled.starts_with(&rendered));
        assert_eq!(
            assembled
                .matches("Return your response as a single JSON object")
                .count(),
            1
        );
    }

    #[test]
    fn assemble_case_prompt_array_uses_batch_count() {
        let case = PromptCase {
            id: "array".to_string(),
            topic: "Rust".to_string(),
            template: None,
            compression: "strong".to_string(),
            mode: "array".to_string(),
            batch_count: 7,
            existing_titles: Vec::new(),
            dismissed_titles: Vec::new(),
            known_items: Vec::new(),
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
            expected: "array prompt".to_string(),
        };

        let prompt = assemble_case_prompt(&case);

        assert!(prompt.contains("Write 7 distinct, non-overlapping cards for this load."));
        assert!(prompt.contains("\"cards\""));
    }

    #[test]
    fn generated_scorecard_row_records_content_words_and_all_token_metrics() {
        let case = test_case("metrics");
        let card = GeneratedCard {
            title: "Three word title".to_string(),
            full_content: "one two three four".to_string(),
            compressed_content: "one two".to_string(),
            use_image: true,
            image_query: "metrics illustration".to_string(),
            usage: crate::llm::transport::TokenUsage {
                prompt_tokens: 11,
                completion_tokens: 22,
                total_tokens: 33,
            },
        };

        let row = scorecard_row(&case, &PromptOutcome::Generated(card), 44);

        assert_eq!(row.title_words, Some(3));
        assert_eq!(row.full_content_words, Some(4));
        assert_eq!(row.compressed_content_words, Some(2));
        assert_eq!(row.prompt_tokens, 11);
        assert_eq!(row.completion_tokens, 22);
        assert_eq!(row.total_tokens, 33);
    }

    #[test]
    fn error_rows_redact_the_api_key_and_escape_markdown_cells() {
        let case = test_case("bad|case");
        let error = redact_error("provider rejected secret-key\nwith | details", "secret-key");
        let row = scorecard_row(&case, &PromptOutcome::Failed(error), 44);

        let markdown = scorecard_markdown(std::slice::from_ref(&row));
        let json = serde_json::to_value(&row).expect("row serializes to JSON");

        assert!(markdown.contains("provider rejected [redacted] with \\| details"));
        assert!(!markdown.contains("secret-key"));
        assert_eq!(
            json["error"],
            "provider rejected [redacted]\nwith | details"
        );
    }

    #[tokio::test]
    async fn checkpoints_serialize_every_scorecard_format() {
        let root = temporary_run_root("checkpoint");
        tokio::fs::create_dir_all(&root)
            .await
            .expect("temporary run directory is created");
        let row = scorecard_row(
            &test_case("checkpoint"),
            &PromptOutcome::Failed("provider error".to_string()),
            9,
        );

        write_scorecard(root.to_str().expect("UTF-8 temporary path"), &[row])
            .await
            .expect("checkpoint succeeds");

        let markdown = tokio::fs::read_to_string(root.join("scorecard.md"))
            .await
            .expect("markdown checkpoint is readable");
        let json = tokio::fs::read_to_string(root.join("scorecard.json"))
            .await
            .expect("JSON checkpoint is readable");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON is valid");

        assert!(markdown.contains("provider error"));
        assert_eq!(parsed.as_array().map(Vec::len), Some(1));
        tokio::fs::remove_dir_all(root)
            .await
            .expect("temporary run directory is removed");
    }

    #[tokio::test]
    async fn run_directory_creation_uses_collision_free_suffixes() {
        let root = temporary_run_root("run-dir");
        let first = create_unique_run_dir(&root, "20260822T010203.004Z")
            .await
            .expect("first run directory is created");
        let second = create_unique_run_dir(&root, "20260822T010203.004Z")
            .await
            .expect("second run directory is created");

        assert_eq!(
            first.file_name().and_then(|name| name.to_str()),
            Some("20260822T010203.004Z-prompts")
        );
        assert_eq!(
            second.file_name().and_then(|name| name.to_str()),
            Some("20260822T010203.004Z-prompts-001")
        );
        tokio::fs::remove_dir_all(root)
            .await
            .expect("temporary run directory is removed");
    }

    #[tokio::test]
    async fn dry_run_returns_zero_contains_english_grammar_and_use_image() {
        let runs_before = run_dir_snapshot();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "prompts".to_string(),
                "--dry-run".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 0);
        assert!(
            stderr.is_empty(),
            "prompt dry-run stderr must be empty: {stderr:?}"
        );
        let output = String::from_utf8(stdout).expect("stdout must be UTF-8");
        assert!(
            output.contains("English Grammar"),
            "prompt dry-run must print English Grammar: {output}"
        );
        assert!(
            output.contains("use_image"),
            "prompt dry-run must print the assembled JSON shape containing use_image: {output}"
        );
        assert!(
            output.contains("prompt_length:"),
            "prompt dry-run must print prompt lengths: {output}"
        );
        assert_eq!(
            run_dir_snapshot(),
            runs_before,
            "prompt dry-run must not create files under lab/runs/"
        );
    }

    #[tokio::test]
    async fn live_without_api_key_exits_two_and_names_the_env_var() {
        let runs_before = run_dir_snapshot();
        let previous = std::env::var_os(API_KEY_ENV);
        // SAFETY: this test must guarantee that the live path cannot see a
        // developer key, and no other test in this crate uses this env var.
        unsafe {
            std::env::remove_var(API_KEY_ENV);
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec!["run".to_string(), "prompts".to_string()],
            &mut stdout,
            &mut stderr,
        )
        .await;

        // Restore whatever the developer environment had before the test.
        // SAFETY: the previous value is a valid environment string.
        unsafe {
            match previous {
                Some(value) => std::env::set_var(API_KEY_ENV, value),
                None => std::env::remove_var(API_KEY_ENV),
            }
        }

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(
            stderr.contains(API_KEY_ENV),
            "missing-key error must name `{API_KEY_ENV}`: {stderr}"
        );
        assert!(
            stderr.contains("live prompt runs require"),
            "missing-key error must be the live-env error: {stderr}"
        );
        assert_eq!(
            run_dir_snapshot(),
            runs_before,
            "missing-key live run must not create files under lab/runs/"
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

    fn test_case(id: &str) -> PromptCase {
        PromptCase {
            id: id.to_string(),
            topic: "Rust".to_string(),
            template: None,
            compression: "strong".to_string(),
            mode: "one_shot".to_string(),
            batch_count: 1,
            existing_titles: Vec::new(),
            dismissed_titles: Vec::new(),
            known_items: Vec::new(),
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
            expected: "prompt test".to_string(),
        }
    }

    fn temporary_run_root(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "denpie-prompts-{label}-{}-{nonce}",
            std::process::id()
        ))
    }
}
