//! Document models for grounding sources.
use crate::api_v1;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct DocumentInfo {
    pub(crate) id: i64,
    #[serde(default)]
    pub(crate) topic_ids: Vec<i64>,
    pub(crate) source_type: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) created_at: String,
}

impl From<api_v1::DocumentRow> for DocumentInfo {
    fn from(d: api_v1::DocumentRow) -> Self {
        Self {
            id: d.id,
            topic_ids: d.topic_ids,
            source_type: d.source_type,
            title: d.title,
            url: d.url,
            created_at: d.created_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct DocumentDetail {
    pub(crate) id: i64,
    pub(crate) source_type: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) content: String,
    pub(crate) created_at: String,
}

impl From<api_v1::DocumentDetailView> for DocumentDetail {
    fn from(d: api_v1::DocumentDetailView) -> Self {
        Self {
            id: d.id,
            source_type: d.source_type,
            title: d.title,
            url: d.url,
            content: d.content,
            created_at: d.created_at,
        }
    }
}
