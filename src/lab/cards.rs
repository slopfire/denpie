//! Repeatable-card fixture gallery runner. There is no network path here:
//! both dry-run and gallery generation only read the checked-in JSON pack
//! and write static HTML/JSON artifacts under `lab/runs/`.

use std::io::{ErrorKind, Write};

use crate::lab::cases::{CardFixture, load_card_cases};

pub(crate) async fn run_cards(
    cases_path: &str,
    dry_run: bool,
    stdout: &mut dyn Write,
) -> Result<(), String> {
    let cases = load_card_cases(cases_path)?;

    if dry_run {
        write!(stdout, "{}", dry_run_plan(cases_path, &cases))
            .map_err(|error| format!("failed to print card dry-run plan: {error}"))?;
        return Ok(());
    }

    let run_dir = create_run_dir().await?;

    let html_path = format!("{run_dir}/gallery.html");
    let json_path = format!("{run_dir}/gallery.json");

    tokio::fs::write(&html_path, gallery_html(&cases))
        .await
        .map_err(|error| format!("failed to write `{html_path}`: {error}"))?;

    let json = serde_json::to_string_pretty(&cases)
        .map_err(|error| format!("failed to serialize card gallery JSON: {error}"))?;
    tokio::fs::write(&json_path, json)
        .await
        .map_err(|error| format!("failed to write `{json_path}`: {error}"))?;

    writeln!(stdout, "gallery: {html_path}")
        .map_err(|error| format!("failed to print card gallery path: {error}"))?;
    writeln!(stdout, "gallery.json: {json_path}")
        .map_err(|error| format!("failed to print card gallery JSON path: {error}"))?;
    writeln!(stdout, "production UI: just lab-cards-ui")
        .map_err(|error| format!("failed to print card lab UI command: {error}"))?;
    Ok(())
}

pub(crate) fn dry_run_plan(cases_path: &str, cases: &[CardFixture]) -> String {
    let mut plan =
        format!("bench: cards\nmode: dry-run (no run artifacts)\ncases: {cases_path}\nplan:\n");
    for case in cases {
        plan.push_str(&format!(
            "  [{id}] topic: {topic} status: {status} pinned: {pinned} pending_count: {pending} review_message: {review_message}\n",
            id = case.id,
            topic = case.topic_name,
            status = case.status,
            pinned = case.pinned,
            pending = case.pending_count,
            review_message = if case.review_message.is_some() {
                "set"
            } else {
                "unset"
            },
        ));
    }
    plan.push_str(&format!("{} fixtures (0 gallery artifacts)\n", cases.len()));
    plan
}

pub(crate) fn gallery_html(cases: &[CardFixture]) -> String {
    let mut html = String::from(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Repeatable-card fixture catalog</title>\n\
         <style>\n\
         :root{color-scheme:light dark;--bg:#f5f2ea;--surface:#fffdf7;--ink:#24211a;--muted:#6f6757;--line:#ddd5c4;--accent:#3f6fb5;--pinned:#9a6700;--reviewed:#1a7f55;--pending:#b04a1f;--error:#b42318}\n\
         *{box-sizing:border-box}\n\
         body{margin:0;background:var(--bg);color:var(--ink);font:15px/1.55 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}\n\
         .wrap{max-width:1180px;margin:0 auto;padding:32px 20px 64px}\n\
         header{border-bottom:1px solid var(--line);padding-bottom:18px;margin-bottom:24px}\n\
         h1{font-size:28px;line-height:1.2;margin:0 0 8px}\n\
         header p{color:var(--muted);margin:0}\n\
         .gallery{display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:20px}\n\
         .fixture{background:var(--surface);border:1px solid var(--line);border-radius:14px;padding:18px;display:flex;flex-direction:column;gap:14px;box-shadow:0 1px 2px rgb(36 33 26 / 6%)}\n\
         .fixture-head{display:flex;flex-direction:column;gap:10px}\n\
         .eyebrow{font-size:12px;font-weight:700;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);margin:0}\n\
         h2{font-size:20px;line-height:1.3;margin:0;overflow-wrap:anywhere}\n\
         .topic{font-weight:600;color:var(--accent);overflow-wrap:anywhere}\n\
         .badges{display:flex;flex-wrap:wrap;gap:6px}\n\
         .badge{display:inline-flex;align-items:center;border:1px solid var(--line);border-radius:999px;padding:3px 9px;font-size:12px;font-weight:600;line-height:1.4;background:#faf7f0;color:var(--ink)}\n\
         .badge-status-active{border-color:color-mix(in srgb,var(--pending) 45%,transparent);color:var(--pending)}\n\
         .badge-status-reviewed{border-color:color-mix(in srgb,var(--reviewed) 45%,transparent);color:var(--reviewed)}\n\
         .badge-pinned{color:var(--pinned)}\n\
         .badge-review{border-color:color-mix(in srgb,var(--reviewed) 45%,transparent);color:var(--reviewed)}\n\
         .badge-error{border-color:color-mix(in srgb,var(--error) 45%,transparent);color:var(--error)}\n\
         .badge-stack{background:#eef3fa;border-color:#c6d5ea}\n\
         .review-message{border-left:3px solid var(--reviewed);background:#edf8f2;border-radius:8px;padding:10px 12px;white-space:pre-wrap;overflow-wrap:anywhere}\n\
         .content-grid{display:grid;gap:10px}\n\
         .content-panel{border:1px solid var(--line);border-radius:10px;padding:10px 12px;background:#fff}\n\
         .content-panel h3{margin:0 0 6px;font-size:12px;letter-spacing:.07em;text-transform:uppercase;color:var(--muted)}\n\
         .card-text{white-space:pre-wrap;overflow-wrap:anywhere;margin:0}\n\
         .notes{color:var(--muted);font-size:13px;margin:0;border-top:1px dashed var(--line);padding-top:10px;white-space:pre-wrap;overflow-wrap:anywhere}\n\
         @media (prefers-color-scheme:dark){:root{--bg:#181611;--surface:#24211a;--ink:#ede7d8;--muted:#aaa08c;--line:#3d382c;--accent:#8db5e8;--pinned:#e0b04f;--reviewed:#62c99a;--pending:#ee8a5e;--error:#f0857f}.badge,.content-panel{background:#2a261e}.badge-stack{background:#263040;border-color:#3d5573}.review-message{background:#1e352b}}\n\
         </style>\n\
         </head>\n\
         <body>\n\
         <div class=\"wrap\">\n\
         <header>\n\
         <h1>Repeatable-card fixture catalog</h1>\n\
         <p>Checked-in data states for the repeatable flow card. Use <code>just lab-cards-ui</code> to render these fixtures with the production Yew component.</p>\n\
         </header>\n\
         <main class=\"gallery\">\n",
    );

    for case in cases {
        html.push_str(&gallery_section(case));
    }

    html.push_str("</main>\n</div>\n</body>\n</html>\n");
    html
}

fn gallery_section(case: &CardFixture) -> String {
    let id = escape_html(&case.id);
    let topic = escape_html(&case.topic_name);
    let title = escape_html(&case.title);
    let tipcard_type = escape_html(&case.tipcard_type);
    let status = escape_html(&case.status);
    let full_content = escape_html(&case.full_content);
    let compressed_content = escape_html(&case.compressed_content);
    let notes = escape_html(&case.notes);
    let stack_layers = stack_layers(&case.tipcard_type, case.pending_count);

    let mut badges = format!(
        "<span class=\"badge badge-status badge-status-{status}\">status: {status}</span>\n\
         <span class=\"badge badge-type\">type: {tipcard_type}</span>\n",
    );
    badges.push_str(&format!(
        "<span class=\"badge badge-pending\">pending: {}</span>\n",
        case.pending_count
    ));
    let pinned_class = if case.pinned { " badge-pinned" } else { "" };
    badges.push_str(&format!(
        "<span class=\"badge{pinned_class}\">pinned: {}</span>\n",
        case.pinned
    ));
    if let Some(message) = &case.review_message {
        let message = escape_html(message);
        badges.push_str(&format!(
            "<span class=\"badge badge-review\">review: {message}</span>\n"
        ));
    } else {
        badges.push_str("<span class=\"badge\">review: none</span>\n");
    }
    if case
        .full_content
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("llm error:")
    {
        badges.push_str("<span class=\"badge badge-error\">llm error</span>\n");
    }
    badges.push_str(&format!(
        "<span class=\"badge badge-stack\">stack layers: {stack_layers}</span>\n"
    ));

    let mut section = format!(
        "<section class=\"fixture\" data-fixture-id=\"{id}\" data-stack-layers=\"{stack_layers}\">\n\
         <div class=\"fixture-head\">\n\
         <p class=\"eyebrow\">{id}</p>\n\
         <h2>{title}</h2>\n\
         <div class=\"topic\">{topic}</div>\n\
         <div class=\"badges\">\n{badges}\
         </div>\n\
         </div>\n"
    );

    if let Some(message) = &case.review_message {
        let message = escape_html(message);
        section.push_str(&format!(
            "<div class=\"review-message\" data-review-message=\"true\">{message}</div>\n"
        ));
    }

    section.push_str(&format!(
        "<div class=\"content-grid\">\n\
         <div class=\"content-panel\">\n\
         <h3>Compressed content</h3>\n\
         <div class=\"card-text\">{compressed_content}</div>\n\
         </div>\n\
         <div class=\"content-panel\">\n\
         <h3>Full content</h3>\n\
         <div class=\"card-text\">{full_content}</div>\n\
         </div>\n\
         </div>\n\
         <p class=\"notes\">{notes}</p>\n\
         </section>\n"
    ));
    section
}

pub(crate) fn stack_layers(tipcard_type: &str, pending_count: u32) -> usize {
    if tipcard_type == "repeatable_tip" {
        usize::try_from(pending_count.min(3)).unwrap_or(0)
    } else {
        0
    }
}

pub(crate) fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
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
        format!("lab/runs/{stamp}-cards")
    } else {
        format!("lab/runs/{stamp}-cards-{attempt}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lab::run_with;

    #[tokio::test]
    async fn dry_run_returns_zero_prints_all_seven_ids_and_writes_nothing() {
        let runs_before = run_dir_snapshot();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            vec![
                "run".to_string(),
                "cards".to_string(),
                "--dry-run".to_string(),
            ],
            &mut stdout,
            &mut stderr,
        )
        .await;

        assert_eq!(code, 0);
        assert!(
            stderr.is_empty(),
            "card dry-run stderr must be empty: {stderr:?}"
        );
        let output = String::from_utf8(stdout).expect("stdout must be UTF-8");
        for id in [
            "active",
            "pinned",
            "reviewed-hold",
            "await-refill",
            "daily-complete",
            "stacked",
            "llm-error",
        ] {
            assert!(output.contains(id), "dry-run must print `{id}`: {output}");
        }
        assert!(
            output.contains("review_message: set"),
            "dry-run must mark fixtures with a review_message: {output}"
        );
        assert!(
            output.contains("review_message: unset"),
            "dry-run must mark fixtures without a review_message: {output}"
        );
        assert_eq!(
            run_dir_snapshot(),
            runs_before,
            "dry-run must not create files under lab/runs/"
        );
    }

    #[test]
    fn gallery_escape_helper_escapes_script_tags() {
        let escaped = escape_html("<script>alert('x')</script>");
        assert!(!escaped.contains("<script>"), "escaped HTML: {escaped}");
        assert!(
            escaped.contains("&lt;script&gt;"),
            "escaped HTML: {escaped}"
        );
        assert!(escaped.contains("&#39;"), "escaped HTML: {escaped}");
    }

    #[test]
    fn gallery_html_escapes_fixture_text() {
        let fixture = CardFixture {
            id: "xss".to_string(),
            topic_name: "<b>Topic</b>".to_string(),
            title: "<script>alert('x')</script>".to_string(),
            full_content: "LLM Error: <img src=x onerror=alert(1)>".to_string(),
            compressed_content: "compact & \"quoted\"".to_string(),
            tipcard_type: "repeatable_tip".to_string(),
            status: "active".to_string(),
            pinned: false,
            pending_count: 4,
            review_message: Some("<em>saved</em>".to_string()),
            notes: "script <script> must not execute".to_string(),
        };

        let html = gallery_html(&[fixture]);

        assert!(!html.contains("<script>"), "gallery HTML: {html}");
        assert!(html.contains("&lt;script&gt;"), "gallery HTML: {html}");
        assert!(!html.contains("<b>Topic</b>"), "gallery HTML: {html}");
        assert!(!html.contains("<em>saved</em>"), "gallery HTML: {html}");
        assert!(html.contains("stack layers: 3"), "gallery HTML: {html}");
    }

    #[test]
    fn stack_layers_match_pending_count_capped_at_three() {
        assert_eq!(stack_layers("repeatable_tip", 0), 0);
        assert_eq!(stack_layers("repeatable_tip", 1), 1);
        assert_eq!(stack_layers("repeatable_tip", 3), 3);
        assert_eq!(stack_layers("repeatable_tip", 8), 3);
        assert_eq!(stack_layers("casual_tip", 8), 0);
    }

    #[test]
    fn run_dir_candidates_are_distinct_after_a_collision() {
        let base = run_dir_candidate("20260822T000000.000Z", 0);
        let retry = run_dir_candidate("20260822T000000.000Z", 1);

        assert_eq!(base, "lab/runs/20260822T000000.000Z-cards");
        assert_eq!(retry, "lab/runs/20260822T000000.000Z-cards-1");
        assert_ne!(base, retry);
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
}
