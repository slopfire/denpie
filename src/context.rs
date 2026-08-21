use std::collections::HashSet;

use axum::http::StatusCode;

use crate::{AppState, db::repositories::tipcards};

/// Newest rows loaded from the topic. Labeled titles in this window are kept
/// even when they fall outside the recent unlabeled cap.
const MAX_CONTEXT_TITLES: usize = 80;
const MAX_CONTEXT_TITLE_CHARS: usize = 100;
/// How many recent titles without a review label are sent. Labeled titles
/// (`known` / `hard` / `skip`) from the fetch window are always included.
const MAX_RECENT_UNLABELED: usize = 24;

const CONTEXT_LEGEND: &str = "Already covered — do not duplicate. [known]=slightly more advanced; [hard]=easier prerequisite; [skip]=avoid similar:";

#[derive(Debug, Default)]
pub struct CardContext {
    existing_titles: Vec<String>,
    dismissed_titles: Vec<String>,
    known_items: Vec<String>,
    difficult_items: Vec<String>,
    uninterested_items: Vec<String>,
}

impl CardContext {
    pub(crate) fn from_parts(
        existing_titles: Vec<String>,
        dismissed_titles: Vec<String>,
        known_items: Vec<String>,
        difficult_items: Vec<String>,
        uninterested_items: Vec<String>,
    ) -> Self {
        Self {
            existing_titles,
            dismissed_titles,
            known_items,
            difficult_items,
            uninterested_items,
        }
    }

    pub fn render_existing(&self) -> String {
        render_titles(&self.existing_titles)
    }

    pub fn render_dismissed(&self) -> String {
        render_titles(&self.dismissed_titles)
    }

    pub fn existing_titles(&self) -> &[String] {
        &self.existing_titles
    }

    pub fn render_all(&self) -> String {
        let lines = self.labeled_title_lines();
        if lines.is_empty() {
            return String::new();
        }
        format!("{CONTEXT_LEGEND}\n{}", render_titles(&lines))
    }

    fn labeled_title_lines(&self) -> Vec<String> {
        let known: HashSet<&str> = self.known_items.iter().map(String::as_str).collect();
        let hard: HashSet<&str> = self.difficult_items.iter().map(String::as_str).collect();
        let skip: HashSet<&str> = self
            .uninterested_items
            .iter()
            .chain(self.dismissed_titles.iter())
            .map(String::as_str)
            .collect();
        let label_of = |title: &str| -> Option<&'static str> {
            if skip.contains(title) {
                Some("skip")
            } else if hard.contains(title) {
                Some("hard")
            } else if known.contains(title) {
                Some("known")
            } else {
                None
            }
        };
        let format_line = |title: &str| match label_of(title) {
            Some(label) => format!("{title} [{label}]"),
            None => title.to_string(),
        };

        let mut seen = HashSet::new();
        let mut lines = Vec::new();
        let mut unlabeled_kept = 0;
        for title in &self.existing_titles {
            let labeled = label_of(title).is_some();
            if !labeled {
                if unlabeled_kept >= MAX_RECENT_UNLABELED {
                    continue;
                }
                unlabeled_kept += 1;
            }
            if seen.insert(title.as_str()) {
                lines.push(format_line(title));
            }
        }
        for title in self
            .dismissed_titles
            .iter()
            .chain(self.known_items.iter())
            .chain(self.difficult_items.iter())
            .chain(self.uninterested_items.iter())
        {
            if seen.insert(title.as_str()) {
                lines.push(format_line(title));
            }
        }
        lines
    }
}

/// Recent titles and review labels for this topic. Card bodies stay out of
/// the generation prompt so a large backlog does not dominate the token budget.
pub async fn load_card_context(
    state: &AppState,
    user_id: &str,
    topic_id: i64,
    tipcard_type: &str,
) -> Result<CardContext, (StatusCode, String)> {
    let rows = tipcards::list_context_titles(
        &state.db,
        user_id,
        topic_id,
        tipcard_type,
        MAX_CONTEXT_TITLES as i64,
    )
    .await
    .map_err(|err| err.into_status_body())?;

    let mut context = CardContext::default();
    for row in rows {
        if row.feedback == "superseded" {
            continue;
        }
        ingest_title(&mut context, &row.title, &row.status, &row.feedback);
    }

    Ok(context)
}

pub fn render_generation_prompt(topic: &str, template: &str, context: &CardContext) -> String {
    let all_context = context.render_all();
    let existing_context = context.render_existing();
    let dismissed_context = context.render_dismissed();

    let mut prompt = template
        .replace("{topic}", topic)
        .replace("{context}", &all_context)
        .replace("{existing_cards}", &existing_context)
        .replace("{dismissed_cards}", &dismissed_context);

    let template_has_context_slot = template.contains("{context}")
        || template.contains("{existing_cards}")
        || template.contains("{dismissed_cards}");
    if !template_has_context_slot && !all_context.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(&all_context);
    }

    prompt
}

fn ingest_title(context: &mut CardContext, title: &str, status: &str, feedback: &str) {
    let title = compact_text(title, MAX_CONTEXT_TITLE_CHARS);
    if title.is_empty() {
        return;
    }
    if status == "dismissed" {
        context.dismissed_titles.push(title.clone());
    } else {
        context.existing_titles.push(title.clone());
    }
    match feedback {
        "learned" | "known" => context.known_items.push(title),
        "again" | "too_difficult" => context.difficult_items.push(title),
        "not_interested" => context.uninterested_items.push(title),
        _ => {}
    }
}

fn render_titles(titles: &[String]) -> String {
    titles
        .iter()
        .enumerate()
        .map(|(idx, title)| format!("{}. {}", idx + 1, title))
        .collect::<Vec<_>>()
        .join("\n")
}

fn compact_text(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_prompt_replaces_context_placeholders() {
        let context = CardContext {
            existing_titles: vec!["Borrow checker basics".to_string()],
            dismissed_titles: vec!["Cargo aliases".to_string()],
            known_items: Vec::new(),
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
        };

        let prompt = render_generation_prompt(
            "Rust",
            "Topic {topic}\nExisting:\n{existing_cards}\nDismissed:\n{dismissed_cards}",
            &context,
        );

        assert!(prompt.contains("Topic Rust"));
        assert!(prompt.contains("1. Borrow checker basics"));
        assert!(prompt.contains("1. Cargo aliases"));
    }

    #[test]
    fn render_prompt_appends_context_when_template_has_no_placeholder() {
        let context = CardContext {
            existing_titles: vec![
                "Iterator adaptors".to_string(),
                "Unreviewed slices card".to_string(),
            ],
            dismissed_titles: Vec::new(),
            known_items: vec!["Iterator adaptors".to_string()],
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
        };

        let prompt = render_generation_prompt("Rust", "Give a tip about {topic}.", &context);

        assert!(prompt.contains("Give a tip about Rust."));
        assert!(prompt.contains("slightly more advanced"));
        assert!(prompt.contains("Iterator adaptors [known]"));
        assert!(prompt.contains("Unreviewed slices card"));
        assert_eq!(prompt.matches("Iterator adaptors").count(), 1);
    }

    #[test]
    fn render_prompt_distinguishes_skip_reasons() {
        let context = CardContext {
            existing_titles: Vec::new(),
            dismissed_titles: vec!["Ownership".to_string(), "Macros".to_string()],
            known_items: vec!["Ownership".to_string()],
            difficult_items: vec!["Lifetimes".to_string()],
            uninterested_items: vec!["Macros".to_string()],
        };

        let prompt = render_generation_prompt("Rust", "Teach {topic}.", &context);

        assert!(prompt.contains("slightly more advanced"));
        assert!(prompt.contains("easier prerequisite"));
        assert!(prompt.contains("avoid similar"));
        assert!(prompt.contains("Ownership [skip]"));
        assert!(prompt.contains("Lifetimes [hard]"));
        assert!(prompt.contains("Macros [skip]"));
        assert!(!prompt.contains("values have one owner"));
    }

    #[test]
    fn context_stores_titles_not_card_bodies() {
        let mut context = CardContext::default();
        ingest_title(&mut context, "Subject-verb agreement", "active", "learned");

        assert_eq!(
            context.existing_titles,
            vec!["Subject-verb agreement".to_string()]
        );
        assert_eq!(
            context.known_items,
            vec!["Subject-verb agreement".to_string()]
        );

        let prompt =
            render_generation_prompt("English grammar", "Write a tip about {topic}.", &context);
        assert!(prompt.contains("Subject-verb agreement [known]"));
        assert_eq!(prompt.matches("Subject-verb agreement").count(), 1);
        assert!(!prompt.contains("The verb must agree"));
    }

    #[test]
    fn unlabeled_titles_are_capped_but_labeled_titles_are_kept() {
        let mut existing_titles = (0..40)
            .map(|index| format!("Recent {index}"))
            .collect::<Vec<_>>();
        existing_titles.push("Old known topic".to_string());
        let context = CardContext {
            existing_titles,
            dismissed_titles: Vec::new(),
            known_items: vec!["Old known topic".to_string()],
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
        };

        let prompt = render_generation_prompt("T", "Tip about {topic}.", &context);
        assert!(prompt.contains("Recent 0"));
        assert!(prompt.contains("Recent 23"));
        assert!(!prompt.contains("Recent 24"));
        assert!(prompt.contains("Old known topic [known]"));
    }

    #[test]
    fn custom_placeholders_are_not_appended_again() {
        let context = CardContext {
            existing_titles: vec!["Borrow checker basics".to_string()],
            dismissed_titles: Vec::new(),
            known_items: vec!["Borrow checker basics".to_string()],
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
        };
        let prompt = render_generation_prompt(
            "Rust",
            "Topic {topic}\nExisting:\n{existing_cards}",
            &context,
        );
        assert!(prompt.contains("1. Borrow checker basics"));
        assert!(!prompt.contains("[known]"));
        assert_eq!(prompt.matches("Borrow checker basics").count(), 1);
    }
}
