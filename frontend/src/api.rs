use crate::i18n::I18n;
use crate::state::{AppAction, AppState, ToastKind};
use gloo_timers::callback::Timeout;
use yew::prelude::*;

fn looks_like_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("fail")
        || lower.contains("error")
        || lower.contains("invalid")
        || lower.contains("unable")
        || lower.contains("denied")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("not found")
        || lower.contains("timeout")
        || lower.contains("panic")
        || lower.starts_with("llm error")
        || lower.contains("api key missing")
}

/// Split a long/error payload into a short headline + expandable detail.
fn split_toast_parts(message: &str) -> (String, Option<String>) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }

    // Prefer first line as summary when multi-line.
    if let Some((head, rest)) = trimmed.split_once('\n') {
        let head = head.trim();
        let rest = rest.trim();
        if !rest.is_empty() {
            return (head.to_string(), Some(rest.to_string()));
        }
    }

    // Long single-line API bodies: keep a short head, rest as detail.
    const HEAD_LIMIT: usize = 120;
    if trimmed.chars().count() > HEAD_LIMIT {
        let mut head = trimmed.chars().take(HEAD_LIMIT).collect::<String>();
        while head.ends_with(|c: char| c.is_whitespace() || c == ',' || c == ':' || c == '{') {
            head.pop();
        }
        head.push('…');
        return (head, Some(trimmed.to_string()));
    }

    (trimmed.to_string(), None)
}

fn toast_timeout_ms(kind: ToastKind, has_detail: bool) -> u32 {
    match kind {
        ToastKind::Error if has_detail => 8_000,
        ToastKind::Error => 5_000,
        ToastKind::Success => 2_400,
        ToastKind::Info => 2_800,
    }
}

pub fn toast(app_state: &UseReducerHandle<AppState>, message: impl Into<String>) {
    let raw = message.into();
    let kind = if looks_like_error(&raw) {
        ToastKind::Error
    } else {
        ToastKind::Info
    };
    let (summary, detail) = split_toast_parts(&raw);
    toast_with(app_state, summary, detail, kind);
}
#[allow(dead_code)]
pub fn toast_error(
    app_state: &UseReducerHandle<AppState>,
    message: impl Into<String>,
    detail: Option<String>,
) {
    let message = message.into();
    let (summary, split_detail) = split_toast_parts(&message);
    let detail = match (detail, split_detail) {
        (Some(d), Some(s)) if d != s => Some(format!("{d}\n{s}")),
        (Some(d), _) => Some(d),
        (None, s) => s,
    };
    toast_with(app_state, summary, detail, ToastKind::Error);
}

#[allow(dead_code)]
pub fn toast_success(app_state: &UseReducerHandle<AppState>, message: impl Into<String>) {
    let (summary, detail) = split_toast_parts(&message.into());
    toast_with(app_state, summary, detail, ToastKind::Success);
}

fn toast_with(
    app_state: &UseReducerHandle<AppState>,
    message: String,
    detail: Option<String>,
    kind: ToastKind,
) {
    let has_detail = detail.as_ref().is_some_and(|d| !d.trim().is_empty());
    app_state.dispatch(AppAction::ShowToast {
        message,
        detail,
        kind,
    });
    let state = app_state.clone();
    Timeout::new(toast_timeout_ms(kind, has_detail), move || {
        state.dispatch(AppAction::HideToast);
    })
    .forget();
}

pub fn toast_key(app_state: &UseReducerHandle<AppState>, i18n: &I18n, key: &str) {
    toast(app_state, i18n.t(key));
}

#[cfg(test)]
mod tests {
    use super::{looks_like_error, split_toast_parts};

    #[test]
    fn splits_multiline_error_into_summary_and_detail() {
        let (summary, detail) = split_toast_parts("Failed to save\nHTTP 500 body here");
        assert_eq!(summary, "Failed to save");
        assert_eq!(detail.as_deref(), Some("HTTP 500 body here"));
    }

    #[test]
    fn long_single_line_becomes_truncated_summary() {
        let long = "x".repeat(200);
        let (summary, detail) = split_toast_parts(&long);
        assert!(summary.ends_with('…'));
        assert!(summary.chars().count() <= 121);
        assert_eq!(detail.as_deref(), Some(long.as_str()));
    }

    #[test]
    fn detects_errorish_messages() {
        assert!(looks_like_error("Failed to parse settings response"));
        assert!(looks_like_error("LLM Error: HTTP 401"));
        assert!(!looks_like_error("Profile refreshed"));
    }
}
