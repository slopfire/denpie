pub const DEFAULT_TOPIC_ICON: &str = "lucide:tag";

pub fn display_icon(icon_id: &str) -> &str {
    if icon_id.trim().is_empty() {
        DEFAULT_TOPIC_ICON
    } else {
        icon_id
    }
}
