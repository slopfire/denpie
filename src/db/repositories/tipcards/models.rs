#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct TipcardRow {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub color_hue: Option<i64>,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub state_data: String,
    pub repeats: i64,
    pub pinned: i64,
}

#[derive(Clone, Debug)]
pub struct ScheduledCardRecord {
    pub id: i64,
    pub full_content: String,
    pub compressed_content: String,
    pub pinned: bool,
    pub image_data: String,
}

#[derive(Clone, Debug)]
pub struct TipcardInfoRecord {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub topic_color: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    #[allow(dead_code)]
    pub state_data: String,
    pub pinned: bool,
    pub repeats: u32,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct TipcardImageRecord {
    pub id: i64,
    pub position: i64,
    pub storage_path: String,
    pub mime_type: String,
    pub byte_size: i64,
}

#[derive(Clone, Debug)]
pub struct FlowCardRecord {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub topic_color: String,
    pub title: String,
    pub compressed_content: String,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    #[allow(dead_code)]
    pub state_data: String,
    pub pinned: bool,
    pub repeats: u32,
    pub image_count: i64,
}

#[derive(Clone, Debug, Default)]
pub struct TipcardFilter {
    pub q: Option<String>,
    pub status: Option<String>,
    pub topic: Option<String>,
    pub tipcard_type: Option<String>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct CardContextTitleRecord {
    pub title: String,
    pub status: String,
}

pub struct CreateManualParams<'a> {
    pub user_id: &'a str,
    pub topic_id: i64,
    pub tipcard_type: &'a str,
    pub title: &'a str,
    pub full_content: &'a str,
    pub compressed_content: &'a str,
    pub image_data_json: &'a str,
}
