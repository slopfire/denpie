//! Opt-in research runner. Everything in here is explicitly outside the
//! server, `just test`, `just verify`, and `just ci`. Live image/prompt lab
//! runs need network and are launched by hand with `just lab`; the cards
//! fixture gallery is local-only.

mod cards;
mod cases;
mod compare;
mod images;
mod prompts;

use std::io::Write;

use tracing_subscriber::EnvFilter;

use crate::domain::grounding::ImageStrategy;

const USAGE: &str = "\
denpie-lab — opt-in research runner (never part of just test / verify / ci)

Usage:
  denpie-lab
  denpie-lab list
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
  run images --concurrency <n>         Parallel live jobs (default 5; Playwright stays 1)

Default image strategy: bing_html. Live jobs share a 90s deadline each.

Prompt bench options:
  run prompts --dry-run                 Print assembled prompts, no LLM calls
  run prompts --offline                 Alias for --dry-run
  run prompts --cases <path>            Case pack (default: lab/cases/prompts/gold.json)
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
        "run" => run_bench(&args[1..], stdout, stderr).await,
        "compare" => run_compare(&args[1..], stdout, stderr),
        _ => usage_error(stderr, &format!("unknown command `{}`", args[0])),
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
            if let Err(message) = images::run_images(
                &options.cases_path,
                &options.strategies,
                options.dry_run,
                options.concurrency,
                stdout,
            )
            .await
            {
                return usage_error(stderr, &message);
            }
            0
        }
        "prompts" => {
            let options = match PromptRunOptions::parse(&args[1..]) {
                Ok(options) => options,
                Err(message) => return usage_error(stderr, &message),
            };
            if let Err(message) =
                prompts::run_prompts(&options.cases_path, options.dry_run, stdout).await
            {
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
}

impl ImageRunOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut cases_path = crate::lab::cases::DEFAULT_GOLD_CASES_PATH.to_string();
        let mut strategies = Vec::new();
        let mut dry_run = false;
        let mut concurrency = images::DEFAULT_CONCURRENCY;
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
        Ok(Self {
            cases_path,
            strategies: unique_strategies,
            dry_run,
            concurrency,
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
}

impl PromptRunOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut cases_path = crate::lab::cases::DEFAULT_PROMPT_CASES_PATH.to_string();
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
