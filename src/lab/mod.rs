//! Opt-in research runner. Everything in here is explicitly outside the
//! server, `just test`, `just verify`, and `just ci`. Live image/prompt lab
//! runs need network and are launched by hand with `just lab`; the cards
//! fixture gallery is local-only.

mod artifacts;
mod cards;
mod cases;
mod compare;
mod images;
mod prompts;
mod review;

use std::io::Write;

use tracing_subscriber::EnvFilter;

use crate::domain::grounding::ImageStrategy;

const USAGE: &str = "\
denpie-lab — opt-in research runner (never part of just test / verify / ci)

Usage:
  denpie-lab
  denpie-lab list
  denpie-lab runs
  denpie-lab show <latest|run-dir|label>
  denpie-lab label <latest|run-dir|label> <name>
  denpie-lab baseline set <name> <latest|run-dir|label>
  denpie-lab baseline show [name]
  denpie-lab review <review.json>
  denpie-lab run <bench> [options]
  denpie-lab compare <baseline-scorecard.json> <candidate-scorecard.json>

Benches:
  images      ready    Image bake-off over the five-card gold set
  algorithms  planned  Scheduler bake-off (no implementation yet)
  prompts     ready    Prompt bake-off (one-shot and array assembly)
  cards       ready    Repeatable-card fixture gallery (offline HTML + JSON)

Image bench options:
  run images --dry-run                 Print the plan without downloading
  run images --offline                 Alias for --dry-run
  run images --strategy <name>         bing_html, bing_playwright, or ddgs_text_og;
                                       repeatable or comma-separated
  run images --strategy all            Expand to all three remote strategies
  run images --cases <path>            Case pack (default: lab/cases/images/gold.json)
  run images --case <id>               Run one case; repeatable
  run images --tag <tag>                Run cases with any selected tag; repeatable
  run images --repeat <n>              Samples per case/strategy (default 1)
  run images --label <name>             Name the run for later show/compare
  run images --resume <run-dir>         Continue missing jobs in a prior run
  run images --retry miss|timeout       Re-run selected failures while resuming
  run images --concurrency <n>         Parallel live jobs (default 5; Playwright stays 1)

Default image strategy: bing_html. Live jobs share a 90s deadline each.

Prompt bench options:
  run prompts --dry-run                 Print assembled prompts, no LLM calls
  run prompts --offline                 Alias for --dry-run
  run prompts --cases <path>            Case pack (default: lab/cases/prompts/gold.json)
  run prompts --case <id>               Run one case; repeatable
  run prompts --tag <tag>                Run cases with any selected tag; repeatable
  run prompts --repeat <n>              Samples per case (default 1)
  run prompts --label <name>             Name the run for later show/compare
  run prompts --resume <run-dir>         Continue missing jobs in a prior run
  run prompts --retry miss|timeout       Re-run selected failures while resuming
  run prompts                           LIVE: one-shot cases call generate_card;
                                        array cases are assembled but not generated

Live prompt runs require DENPIE_LAB_LLM_API_KEY. Optional:
  DENPIE_LAB_LLM_MODEL    (default: google/gemini-3.1-flash)
  DENPIE_LAB_LLM_BASE_URL (default: https://openrouter.ai/api/v1)

Card bench options:
  run cards --dry-run                 Print fixture states without writing lab/runs
  run cards --offline                 Alias for --dry-run
  run cards --cases <path>            Case pack (default: lab/cases/cards/repeatable-states.json)
  run cards                           Write gallery.html + gallery.json (no network)

Compare:
  compare <baseline> <candidate>      Compare two image or two prompt scorecards
                                       (offline; valid comparisons exit 0)
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bench {
    pub(crate) name: &'static str,
    pub(crate) status: &'static str,
    pub(crate) purpose: &'static str,
}

pub(crate) fn list_benches() -> Vec<Bench> {
    vec![
        Bench {
            name: "images",
            status: "ready",
            purpose: "Image bake-off over the five-card gold set",
        },
        Bench {
            name: "algorithms",
            status: "planned",
            purpose: "Scheduler bake-off (no implementation yet)",
        },
        Bench {
            name: "prompts",
            status: "ready",
            purpose: "Prompt bake-off (one-shot and array assembly)",
        },
        Bench {
            name: "cards",
            status: "ready",
            purpose: "Repeatable-card fixture gallery (offline HTML + JSON)",
        },
    ]
}

pub async fn run(args: Vec<String>) -> i32 {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with(args, &mut stdout, &mut stderr).await
}

pub(crate) async fn run_with(
    args: Vec<String>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    init_tracing();

    if args.is_empty() || args[0] == "-h" || args[0] == "--help" || args[0] == "help" {
        let _ = write!(stdout, "{USAGE}");
        return 0;
    }

    match args[0].as_str() {
        "list" => run_list(&args[1..], stdout, stderr),
        "runs" => run_runs(&args[1..], stdout, stderr),
        "show" => run_show(&args[1..], stdout, stderr),
        "label" => run_label(&args[1..], stdout, stderr),
        "baseline" => run_baseline(&args[1..], stdout, stderr),
        "review" => run_review(&args[1..], stdout, stderr),
        "run" => run_bench(&args[1..], stdout, stderr).await,
        "compare" => run_compare(&args[1..], stdout, stderr),
        _ => usage_error(stderr, &format!("unknown command `{}`", args[0])),
    }
}

fn run_review(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let [path] = args else {
        return usage_error(stderr, "`review` requires one exported review.json path");
    };
    match review::render(path) {
        Ok(report) => write!(stdout, "{report}").map_or(2, |()| 0),
        Err(message) => usage_error(stderr, &message),
    }
}

fn run_baseline(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let result = match args {
        [command, name, selector] if command == "set" => artifacts::set_baseline(name, selector),
        [command] if command == "show" || command == "list" => artifacts::show_baselines(None),
        [command, name] if command == "show" => artifacts::show_baselines(Some(name)),
        _ => {
            return usage_error(
                stderr,
                "use `baseline set <name> <run>` or `baseline show [name]`",
            );
        }
    };
    match result {
        Ok(report) => write!(stdout, "{report}").map_or(2, |()| 0),
        Err(message) => usage_error(stderr, &message),
    }
}

fn run_runs(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if !args.is_empty() {
        return usage_error(stderr, "`runs` takes no arguments");
    }
    match artifacts::list_runs() {
        Ok(report) => write!(stdout, "{report}").map_or(2, |()| 0),
        Err(message) => usage_error(stderr, &message),
    }
}

fn run_show(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let [selector] = args else {
        return usage_error(
            stderr,
            "`show` requires one run directory, label, or `latest`",
        );
    };
    match artifacts::show_run(selector) {
        Ok(report) => write!(stdout, "{report}").map_or(2, |()| 0),
        Err(message) => usage_error(stderr, &message),
    }
}

fn run_label(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let [selector, label] = args else {
        return usage_error(stderr, "`label` requires a run selector and label");
    };
    match artifacts::label_run(selector, label) {
        Ok(report) => write!(stdout, "{report}").map_or(2, |()| 0),
        Err(message) => usage_error(stderr, &message),
    }
}

fn run_compare(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let [baseline, candidate] = args else {
        return usage_error(
            stderr,
            "`compare` requires a baseline and candidate scorecard JSON path",
        );
    };

    match compare::render(baseline, candidate) {
        Ok(report) => match write!(stdout, "{report}") {
            Ok(()) => 0,
            Err(_) => 2,
        },
        Err(message) => usage_error(stderr, &message),
    }
}

fn run_list(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    if let Some(unexpected) = args.first() {
        return usage_error(
            stderr,
            &format!("unexpected argument for `list`: `{unexpected}`"),
        );
    }

    if writeln!(stdout, "benches:").is_err() {
        return 2;
    }
    for bench in list_benches() {
        if writeln!(
            stdout,
            "  {:<12}{:<9}{}",
            bench.name, bench.status, bench.purpose
        )
        .is_err()
        {
            return 2;
        }
    }
    0
}

async fn run_bench(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let Some(bench) = args.first() else {
        return usage_error(stderr, "`run` requires a bench name");
    };

    match bench.as_str() {
        "images" => {
            let options = match ImageRunOptions::parse(&args[1..]) {
                Ok(options) => options,
                Err(message) => return usage_error(stderr, &message),
            };
            let config = images::ImageRunConfig {
                cases_path: &options.cases_path,
                strategies: &options.strategies,
                dry_run: options.dry_run,
                concurrency: options.concurrency,
                case_ids: &options.case_ids,
                tags: &options.tags,
                repeat: options.repeat,
                label: options.label.as_deref(),
                resume: options.resume.as_deref(),
                retry: options.retry,
            };
            if let Err(message) = images::run_images(&config, stdout).await {
                return usage_error(stderr, &message);
            }
            0
        }
        "prompts" => {
            let options = match PromptRunOptions::parse(&args[1..]) {
                Ok(options) => options,
                Err(message) => return usage_error(stderr, &message),
            };
            let config = prompts::PromptRunConfig {
                cases_path: &options.cases_path,
                dry_run: options.dry_run,
                case_ids: &options.case_ids,
                tags: &options.tags,
                repeat: options.repeat,
                label: options.label.as_deref(),
                resume: options.resume.as_deref(),
                retry: options.retry,
            };
            if let Err(message) = prompts::run_prompts(&config, stdout).await {
                return usage_error(stderr, &message);
            }
            0
        }
        "cards" => {
            let options = match CardRunOptions::parse(&args[1..]) {
                Ok(options) => options,
                Err(message) => return usage_error(stderr, &message),
            };
            if let Err(message) =
                cards::run_cards(&options.cases_path, options.dry_run, stdout).await
            {
                return usage_error(stderr, &message);
            }
            0
        }
        "algorithms" => usage_error(stderr, &format!("bench `{bench}` is not implemented")),
        _ => usage_error(stderr, &format!("unknown bench `{bench}`")),
    }
}

#[derive(Debug)]
struct ImageRunOptions {
    cases_path: String,
    strategies: Vec<ImageStrategy>,
    dry_run: bool,
    concurrency: usize,
    case_ids: Vec<u64>,
    tags: Vec<String>,
    repeat: usize,
    label: Option<String>,
    resume: Option<String>,
    retry: Option<artifacts::RetryPolicy>,
}

impl ImageRunOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut cases_path = crate::lab::cases::DEFAULT_GOLD_CASES_PATH.to_string();
        let mut strategies = Vec::new();
        let mut dry_run = false;
        let mut concurrency = images::DEFAULT_CONCURRENCY;
        let mut case_ids = Vec::new();
        let mut tags = Vec::new();
        let mut repeat = 1;
        let mut label = None;
        let mut resume = None;
        let mut retry = None;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--dry-run" | "--offline" => dry_run = true,
                "--concurrency" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "`--concurrency` requires a positive integer".to_string())?;
                    if value.starts_with('-') {
                        return Err(format!(
                            "`--concurrency` requires a positive integer, got `{value}`"
                        ));
                    }
                    let parsed = value.parse::<usize>().map_err(|_| {
                        format!("`--concurrency` requires a positive integer, got `{value}`")
                    })?;
                    if parsed == 0 {
                        return Err("`--concurrency` must be at least 1".to_string());
                    }
                    concurrency = parsed;
                    index += 1;
                }
                "--strategy" => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        "`--strategy` requires a value (bing_html, bing_playwright, ddgs_text_og, or all)"
                            .to_string()
                    })?;
                    if value.starts_with('-') {
                        return Err(format!("`--strategy` requires a value, got `{value}`"));
                    }
                    strategies.extend(parse_strategy_value(value)?);
                    index += 1;
                }
                "--cases" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "`--cases` requires a path".to_string())?;
                    if value.starts_with('-') {
                        return Err(format!("`--cases` requires a path, got `{value}`"));
                    }
                    cases_path = value.clone();
                    index += 1;
                }
                "--case" => {
                    let value = required_value(args, index, "--case")?;
                    case_ids.push(value.parse::<u64>().map_err(|_| {
                        format!("`--case` requires an unsigned image case id, got `{value}`")
                    })?);
                    index += 1;
                }
                "--tag" => {
                    tags.push(required_value(args, index, "--tag")?.to_string());
                    index += 1;
                }
                "--repeat" => {
                    repeat = positive_usize(args, index, "--repeat")?;
                    index += 1;
                }
                "--label" => {
                    let value = required_value(args, index, "--label")?;
                    artifacts::validate_label(value)?;
                    label = Some(value.to_string());
                    index += 1;
                }
                "--resume" => {
                    resume = Some(required_value(args, index, "--resume")?.to_string());
                    index += 1;
                }
                "--retry" => {
                    retry = Some(parse_retry(required_value(args, index, "--retry")?)?);
                    index += 1;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown flag `{other}`"));
                }
                other => return Err(format!("unexpected argument `{other}`")),
            }
            index += 1;
        }

        if strategies.is_empty() {
            strategies.push(ImageStrategy::BingHtml);
        }
        let mut unique_strategies = Vec::with_capacity(strategies.len());
        for strategy in strategies {
            if !unique_strategies.contains(&strategy) {
                unique_strategies.push(strategy);
            }
        }
        validate_resume_flags(dry_run, resume.as_deref(), retry)?;
        Ok(Self {
            cases_path,
            strategies: unique_strategies,
            dry_run,
            concurrency,
            case_ids,
            tags,
            repeat,
            label,
            resume,
            retry,
        })
    }
}

#[derive(Debug)]
struct CardRunOptions {
    cases_path: String,
    dry_run: bool,
}

impl CardRunOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut cases_path = crate::lab::cases::DEFAULT_CARD_CASES_PATH.to_string();
        let mut dry_run = false;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--dry-run" | "--offline" => dry_run = true,
                "--cases" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "`--cases` requires a path".to_string())?;
                    if value.starts_with('-') {
                        return Err(format!("`--cases` requires a path, got `{value}`"));
                    }
                    cases_path = value.clone();
                    index += 1;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown flag `{other}`"));
                }
                other => return Err(format!("unexpected argument `{other}`")),
            }
            index += 1;
        }

        Ok(Self {
            cases_path,
            dry_run,
        })
    }
}

#[derive(Debug)]
struct PromptRunOptions {
    cases_path: String,
    dry_run: bool,
    case_ids: Vec<String>,
    tags: Vec<String>,
    repeat: usize,
    label: Option<String>,
    resume: Option<String>,
    retry: Option<artifacts::RetryPolicy>,
}

impl PromptRunOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut cases_path = crate::lab::cases::DEFAULT_PROMPT_CASES_PATH.to_string();
        let mut dry_run = false;
        let mut case_ids = Vec::new();
        let mut tags = Vec::new();
        let mut repeat = 1;
        let mut label = None;
        let mut resume = None;
        let mut retry = None;
        let mut index = 0;

        while index < args.len() {
            match args[index].as_str() {
                "--dry-run" | "--offline" => dry_run = true,
                "--cases" => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| "`--cases` requires a path".to_string())?;
                    if value.starts_with('-') {
                        return Err(format!("`--cases` requires a path, got `{value}`"));
                    }
                    cases_path = value.clone();
                    index += 1;
                }
                "--case" => {
                    case_ids.push(required_value(args, index, "--case")?.to_string());
                    index += 1;
                }
                "--tag" => {
                    tags.push(required_value(args, index, "--tag")?.to_string());
                    index += 1;
                }
                "--repeat" => {
                    repeat = positive_usize(args, index, "--repeat")?;
                    index += 1;
                }
                "--label" => {
                    let value = required_value(args, index, "--label")?;
                    artifacts::validate_label(value)?;
                    label = Some(value.to_string());
                    index += 1;
                }
                "--resume" => {
                    resume = Some(required_value(args, index, "--resume")?.to_string());
                    index += 1;
                }
                "--retry" => {
                    retry = Some(parse_retry(required_value(args, index, "--retry")?)?);
                    index += 1;
                }
                other if other.starts_with('-') => {
                    return Err(format!("unknown flag `{other}`"));
                }
                other => return Err(format!("unexpected argument `{other}`")),
            }
            index += 1;
        }

        validate_resume_flags(dry_run, resume.as_deref(), retry)?;
        Ok(Self {
            cases_path,
            dry_run,
            case_ids,
            tags,
            repeat,
            label,
            resume,
            retry,
        })
    }
}

fn required_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("`{flag}` requires a value"))?;
    if value.starts_with('-') {
        return Err(format!("`{flag}` requires a value, got `{value}`"));
    }
    Ok(value)
}

fn positive_usize(args: &[String], index: usize, flag: &str) -> Result<usize, String> {
    let value = required_value(args, index, flag)?;
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("`{flag}` requires a positive integer, got `{value}`"))?;
    if parsed == 0 {
        return Err(format!("`{flag}` must be at least 1"));
    }
    Ok(parsed)
}

fn parse_retry(value: &str) -> Result<artifacts::RetryPolicy, String> {
    match value {
        "miss" => Ok(artifacts::RetryPolicy::Miss),
        "timeout" => Ok(artifacts::RetryPolicy::Timeout),
        _ => Err(format!(
            "`--retry` must be `miss` or `timeout`, got `{value}`"
        )),
    }
}

fn validate_resume_flags(
    dry_run: bool,
    resume: Option<&str>,
    retry: Option<artifacts::RetryPolicy>,
) -> Result<(), String> {
    if retry.is_some() && resume.is_none() {
        return Err("`--retry` requires `--resume <run-dir>`".to_string());
    }
    if dry_run && resume.is_some() {
        return Err("`--resume` is only available for live runs".to_string());
    }
    Ok(())
}

fn parse_strategy_value(value: &str) -> Result<Vec<ImageStrategy>, String> {
    let mut strategies = Vec::new();
    for part in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        if part == "all" {
            strategies.extend([
                ImageStrategy::BingHtml,
                ImageStrategy::BingPlaywright,
                ImageStrategy::DdgsTextOg,
            ]);
            continue;
        }
        let strategy = match part {
            "bing_html" => ImageStrategy::BingHtml,
            "bing_playwright" => ImageStrategy::BingPlaywright,
            "ddgs_text_og" => ImageStrategy::DdgsTextOg,
            _ => return Err(format!("unknown strategy `{part}`")),
        };
        if !strategies.contains(&strategy) {
            strategies.push(strategy);
        }
    }

    if strategies.is_empty() {
        return Err("`--strategy` requires at least one strategy".to_string());
    }
    Ok(strategies)
}

fn usage_error(stderr: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(stderr, "error: {message}");
    let _ = writeln!(stderr, "Run `denpie-lab --help` for usage.");
    2
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("denpie=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_benches_reports_ready_and_planned_benches() {
        let benches = list_benches();
        assert_eq!(benches.len(), 4);

        for ready in ["images", "prompts", "cards"] {
            let bench = benches
                .iter()
                .find(|bench| bench.name == ready)
                .unwrap_or_else(|| panic!("{ready} bench must be listed"));
            assert_eq!(bench.status, "ready");
        }

        let algorithms = benches
            .iter()
            .find(|bench| bench.name == "algorithms")
            .expect("algorithms bench must be listed");
        assert_eq!(algorithms.status, "planned");
    }

    #[tokio::test]
    async fn run_algorithms_exits_two_not_implemented() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec!["run".to_string(), "algorithms".to_string()],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("not implemented"));
    }

    #[tokio::test]
    async fn run_cards_with_unknown_flag_exits_two() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec!["run".to_string(), "cards".to_string(), "--nope".to_string()],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("unknown flag `--nope`"));
    }

    #[tokio::test]
    async fn run_prompts_with_unknown_flag_exits_two() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "prompts".to_string(),
                "--nope".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("unknown flag `--nope`"));
    }

    #[tokio::test]
    async fn run_images_with_unknown_flag_exits_two() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "images".to_string(),
                "--nope".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("unknown flag `--nope`"));
    }

    #[test]
    fn strategy_all_expands_to_the_three_remote_strategies() {
        assert_eq!(
            parse_strategy_value("all").unwrap(),
            vec![
                ImageStrategy::BingHtml,
                ImageStrategy::BingPlaywright,
                ImageStrategy::DdgsTextOg,
            ]
        );
    }

    #[test]
    fn comma_separated_strategies_are_deduplicated() {
        assert_eq!(
            parse_strategy_value("bing_html,bing_html").unwrap(),
            vec![ImageStrategy::BingHtml]
        );
    }

    #[test]
    fn repeated_strategy_flags_are_deduplicated_globally() {
        let options = ImageRunOptions::parse(&[
            "--strategy".to_string(),
            "bing_html".to_string(),
            "--strategy".to_string(),
            "all".to_string(),
            "--strategy".to_string(),
            "bing_html,ddgs_text_og".to_string(),
        ])
        .expect("valid strategy flags");

        assert_eq!(
            options.strategies,
            vec![
                ImageStrategy::BingHtml,
                ImageStrategy::BingPlaywright,
                ImageStrategy::DdgsTextOg,
            ]
        );
    }

    #[test]
    fn image_iteration_flags_parse_case_repeat_and_label() {
        let options = ImageRunOptions::parse(&[
            "--case".to_string(),
            "270".to_string(),
            "--tag".to_string(),
            "grammar".to_string(),
            "--repeat".to_string(),
            "3".to_string(),
            "--label".to_string(),
            "candidate_3".to_string(),
        ])
        .expect("iteration flags are valid");

        assert_eq!(options.case_ids, vec![270]);
        assert_eq!(options.tags, vec!["grammar"]);
        assert_eq!(options.repeat, 3);
        assert_eq!(options.label.as_deref(), Some("candidate_3"));
    }

    #[test]
    fn prompt_iteration_flags_reject_invalid_repeat_and_label() {
        assert!(
            PromptRunOptions::parse(&["--repeat".to_string(), "0".to_string()])
                .unwrap_err()
                .contains("at least 1")
        );
        assert!(
            PromptRunOptions::parse(&["--label".to_string(), "has spaces".to_string(),])
                .unwrap_err()
                .contains("run labels")
        );
        assert!(
            PromptRunOptions::parse(&["--retry".to_string(), "timeout".to_string(),])
                .unwrap_err()
                .contains("requires `--resume")
        );
    }

    #[tokio::test]
    async fn run_images_rejects_zero_concurrency() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "images".to_string(),
                "--concurrency".to_string(),
                "0".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("`--concurrency` must be at least 1"));
    }
}
