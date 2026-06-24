use crate::domain::topic_visual;

pub(crate) mod models;
pub(crate) mod queries;

mod context_titles;
mod flow;
mod images;
mod info;
mod queue;
mod writes;

pub use context_titles::list_context_titles;
pub use flow::list_flow_cards;
pub use images::{find_image, list_images, list_images_for_cards, replace_image_records};
pub use info::{get_tipcard_info, list_admin, list_filtered};
#[allow(unused_imports)]
pub use models::{
    CardContextTitleRecord, CreateManualParams, FlowCardRecord, ScheduledCardRecord, TipcardFilter,
    TipcardImageRecord, TipcardInfoRecord,
};
pub use queue::{active_card_count, find_daily_topic_cards, find_due_topic_cards};
pub use writes::{create_custom, create_generated, create_manual, delete_with_review, set_pinned};

pub(crate) fn topic_color_from_row(name: &str, color_hue: Option<i64>) -> String {
    topic_visual::resolve_topic_color(color_hue.map(|hue| hue as i32), name)
}
