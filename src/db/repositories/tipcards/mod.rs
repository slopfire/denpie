use crate::domain::topic_visual;

pub(crate) mod models;
pub(crate) mod queries;

mod context_titles;
mod daily_allowances;
mod flow;
mod images;
mod info;
mod pending;
mod queue;
mod writes;

pub use context_titles::list_context_titles;
pub use daily_allowances::{add_extra_cards, extra_cards_in_window};
pub use flow::list_flow_cards;
pub use images::{
    append_image_records, find_image, list_images, list_images_for_cards, replace_image_records,
};
pub use info::{get_tipcard_info, list_admin, list_filtered};
#[allow(unused_imports)]
pub use models::{
    CardContextTitleRecord, CreateManualParams, FlowCardRecord, ScheduledCardRecord, TipcardFilter,
    TipcardImageRecord, TipcardInfoRecord,
};
#[allow(unused_imports)]
pub use pending::{
    count_pending, park_unseen_active_topic_cards, promote_pending_for_empty_topics,
    replace_unseen_with_pending_card, stack_due_repeatable_cards, take_pending_card,
};
pub use queue::{
    active_card_count, count_reviewed_in_window, find_daily_topic_cards, find_due_topic_cards,
    has_active_topic_card,
};
pub use writes::{
    create_custom, create_generated_with_status, create_manual, delete_failed_generation_cards,
    delete_with_review, set_pinned,
};

pub(crate) fn topic_color_from_row(name: &str, color_hue: Option<i64>) -> String {
    topic_visual::resolve_topic_color(color_hue.map(|hue| hue as i32), name)
}
