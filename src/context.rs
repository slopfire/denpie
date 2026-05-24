use axum::http::StatusCode;

use crate::{AppState, db::repositories::tipcards};

//todo
const MAX_CONTEXT_TITLES: usize = 80;

#[derive(Debug, Default)]
pub struct CardContext {
    existing_titles: Vec<String>,
    dismissed_titles: Vec<String>,
}

impl CardContext {
    pub fn render_existing(&self) -> String {
        render_titles(&self.existing_titles)
    }

    pub fn render_dismissed(&self) -> String {
        render_titles(&self.dismissed_titles)
    }

    pub fn render_all(&self) -> String {
        let existing = self.render_existing();
        let dismissed = self.render_dismissed();
        match (existing.is_empty(), dismissed.is_empty()) {
            (true, true) => String::new(),
            (false, true) => format!("Existing cards:\n{existing}"),
            (true, false) => format!("Dismissed cards:\n{dismissed}"),
            (false, false) => {
                format!("Existing cards:\n{existing}\n\nDismissed cards:\n{dismissed}")
            }
        }
    }
}

//todo compressed???
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
        let title = compact_title(&row.title);
        if title.is_empty() {
            continue;
        }
        if row.status == "dismissed" {
            context.dismissed_titles.push(title);
        } else {
            context.existing_titles.push(title);
        }
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

    if !template.contains("{context}")
        && !template.contains("{existing_cards}")
        && !template.contains("{dismissed_cards}")
        && !all_context.is_empty()
    {
        prompt.push_str(
            "\n\nUse this card history as context. Do not duplicate these titles or ideas:\n",
        );
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

fn compact_title(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_prompt_replaces_context_placeholders() {
        let context = CardContext {
            existing_titles: vec!["Borrow checker basics".to_string()],
            dismissed_titles: vec!["Cargo aliases".to_string()],
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
            existing_titles: vec!["Iterator adaptors".to_string()],
            dismissed_titles: Vec::new(),
        };

        let prompt = render_generation_prompt("Rust", "Give a tip about {topic}.", &context);

        assert!(prompt.contains("Give a tip about Rust."));
        assert!(prompt.contains("Do not duplicate"));
        assert!(prompt.contains("Iterator adaptors"));
    }
}
