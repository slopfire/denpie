//! Local image pool models for grounding.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct PoolImageInfo {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) created_at: String,
}
#[derive(Serialize)]
pub(crate) struct AddPoolImageReq {
    pub(crate) image_data: String,
    pub(crate) name: String,
}

#[derive(Serialize)]
pub(crate) struct RenamePoolImageReq {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct RemovePoolImageTagReq {
    pub(crate) id: i64,
    pub(crate) tag: String,
}

#[derive(Serialize)]
pub(crate) struct DeletePoolImageReq {
    pub(crate) id: i64,
}
