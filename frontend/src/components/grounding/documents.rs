//! Document models for grounding sources.
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct DocumentDetail {
    pub(crate) id: i64,
    pub(crate) source_type: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) content: String,
    pub(crate) created_at: String,
}

#[derive(Serialize)]
pub(crate) struct AddDocumentReq {
    pub(crate) topic_ids: Vec<i64>,
    pub(crate) source_type: String,
    pub(crate) title: String,
    pub(crate) url: Option<String>,
    pub(crate) content: String,
}

#[derive(Serialize)]
pub(crate) struct ExploreLinkReq {
    pub(crate) url: String,
}

#[derive(Deserialize)]
pub(crate) struct ExploredLink {
    pub(crate) url: String,
}

#[derive(Serialize)]
pub(crate) struct DeleteDocumentReq {
    pub(crate) id: i64,
}

#[derive(Serialize)]
pub(crate) struct UploadDocumentReq {
    pub(crate) filename: String,
    pub(crate) title: Option<String>,
    pub(crate) data_url: String,
    pub(crate) topic_ids: Vec<i64>,
}

#[derive(Serialize)]
pub(crate) struct AttachDocumentReq {
    pub(crate) topic_id: i64,
}
