//! Local image pool models for grounding.
use crate::api_v1;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct PoolImageInfo {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) created_at: String,
}

impl From<api_v1::PoolImageRow> for PoolImageInfo {
    fn from(img: api_v1::PoolImageRow) -> Self {
        Self {
            id: img.id,
            name: img.name,
            description: img.description,
            tags: img.tags,
            created_at: img.created_at,
        }
    }
}

/// Session JSON — no v1 op for rename.
#[derive(Serialize)]
pub(crate) struct RenamePoolImageReq {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

/// Session JSON — no v1 op for tag removal.
#[derive(Serialize)]
pub(crate) struct RemovePoolImageTagReq {
    pub(crate) id: i64,
    pub(crate) tag: String,
}
