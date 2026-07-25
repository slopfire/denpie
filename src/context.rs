use axum::http::StatusCode;

use crate::{AppState, db::repositories::tipcards};

const MAX_CONTEXT_TITLES: usize = 80;
const MAX_CONTEXT_CONTENT_CHARS: usize = 240;

#[derive(Debug, Default)]
pub struct CardContext {
    existing_titles: Vec<String>,
    dismissed_titles: Vec<String>,
    known_items: Vec<String>,
    difficult_items: Vec<String>,
    uninterested_items: Vec<String>,
}

impl CardContext {
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
        let mut sections = if self.existing_titles.is_empty() {
            Vec::new()
        } else {
            vec![format!(
                "Existing cards (never duplicate these):\n{}",
                render_titles(&self.existing_titles)
            )]
        };
        let feedback = self.render_feedback();
        if !feedback.is_empty() {
            sections.push(feedback);
        }
        sections.join("\n\n")
    }

    fn render_feedback(&self) -> String {
        [
            (
                "Known or learned (choose a nearby concept that is slightly more advanced)",
                &self.known_items,
            ),
            (
                "Too difficult or requested again (choose an easier prerequisite or simpler example)",
                &self.difficult_items,
            ),
            (
                "Not interested (avoid similar subject matter, framing, and examples)",
                &self.uninterested_items,
            ),
        ]
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(heading, items)| format!("{heading}:\n{}", render_titles(items)))
        .collect::<Vec<_>>()
        .join("\n\n")
    }
}

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
        let title = compact_text(&row.title, 100);
        if title.is_empty() {
            continue;
        }
        let content = compact_text(&row.content, MAX_CONTEXT_CONTENT_CHARS);
        let item = if content.is_empty() || content == title {
            title.clone()
        } else {
            format!("{title}: {content}")
        };
        if row.status == "dismissed" {
            context.dismissed_titles.push(title);
        } else {
            context.existing_titles.push(item.clone());
        }
        match row.feedback.as_str() {
            "learned" | "known" => context.known_items.push(item),
            "again" | "too_difficult" => context.difficult_items.push(item),
            "not_interested" => context.uninterested_items.push(item),
            _ => {}
        }
    }

    Ok(context)
}

pub fn render_generation_prompt(topic: &str, template: &str, context: &CardContext) -> String {
    let all_context = context.render_all();
    let existing_context = context.render_existing();
    let dismissed_context = context.render_dismissed();
    let feedback_context = context.render_feedback();

    let mut prompt = template
        .replace("{topic}", topic)
        .replace("{context}", &all_context)
        .replace("{existing_cards}", &existing_context)
        .replace("{dismissed_cards}", &dismissed_context);

    if !template.contains("{context}") && !feedback_context.is_empty() {
        prompt.push_str(
            "\n\nPersonalize the next learning card from this feedback. Infer the learner's current level, but do not claim certainty or merely paraphrase a previous card. Follow each section's direction:\n",
        );
        prompt.push_str(&all_context);
    } else if !template.contains("{context}")
        && !template.contains("{existing_cards}")
        && !template.contains("{dismissed_cards}")
        && !all_context.is_empty()
    {
        prompt.push_str("\n\nDo not duplicate these existing cards or ideas:\n");
        prompt.push_str(&all_context);
    }

    prompt
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
            known_items: vec!["Iterator adaptors: map transforms each item".to_string()],
            difficult_items: Vec::new(),
            uninterested_items: Vec::new(),
        };

        let prompt = render_generation_prompt("Rust", "Give a tip about {topic}.", &context);

        assert!(prompt.contains("Give a tip about Rust."));
        assert!(prompt.contains("slightly more advanced"));
        assert!(prompt.contains("Iterator adaptors"));
        assert!(prompt.contains("Unreviewed slices card"));
    }

    #[test]
    fn render_prompt_distinguishes_skip_reasons() {
        let context = CardContext {
            existing_titles: Vec::new(),
            dismissed_titles: vec!["Ownership".to_string(), "Macros".to_string()],
            known_items: vec!["Ownership: values have one owner".to_string()],
            difficult_items: vec!["Lifetimes: references must remain valid".to_string()],
            uninterested_items: vec!["Macros: token trees".to_string()],
        };

        let prompt = render_generation_prompt("Rust", "Teach {topic}.", &context);

        assert!(prompt.contains("slightly more advanced"));
        assert!(prompt.contains("easier prerequisite"));
        assert!(prompt.contains("avoid similar subject matter"));
    }
}
