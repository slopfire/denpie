use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Deserialize)]
struct TopicIconConfig {
    icons: Vec<String>,
}

fn load_allowlist() -> Vec<String> {
    serde_json::from_str::<TopicIconConfig>(include_str!("../../config/topic_icons.json"))
        .expect("config/topic_icons.json must be valid JSON")
        .icons
}

pub fn allowlist() -> &'static [String] {
    static ALLOWLIST: OnceLock<Vec<String>> = OnceLock::new();
    ALLOWLIST.get_or_init(load_allowlist)
}
