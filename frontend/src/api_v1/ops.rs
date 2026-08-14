//! Typed API v1 operations used by the browser UI.
//!
//! Each function encodes a real `ApiRequest` op, posts `/api/v1`, and maps the
//! protobuf result into UI-friendly shapes (matching prior dashboard JSON types
//! where practical).

use super::client::{ApiError, ApiResult, call_mutation, call_mutation_with_key, call_read};
use crate::pb::{
    self, AddDocumentRequest, AddPoolImageRequest, AttachDocumentTopicRequest,
    ContinueDailyReviewRequest, CreateApiKeyRequest, DeleteByIdRequest, Empty, ExploreLinkRequest,
    ForceDailyRefreshOutcome, ForceDailyRefreshRequest, GetByIdRequest, ListFlowCardsRequest,
    PinTipcardRequest, ReviewActionValue, ReviewAndAdvanceRequest, TipcardTypeValue, TipsRequestV1,
    UpdateSettingsRequest, UpdateTopicRequest, UploadDocumentRequest, api_request, api_response,
};
use base64::Engine;
use serde::{Deserialize, Serialize};

// ---- UI-facing result types (serde-free; built from protobuf) ----

#[derive(Clone, Debug, PartialEq)]
pub struct FlowCardPage {
    pub cards: Vec<FlowCardSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CardSource {
    pub document_id: i64,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
}

impl From<pb::CardSource> for CardSource {
    fn from(source: pb::CardSource) -> Self {
        Self {
            document_id: source.document_id,
            source_type: source.source_type,
            title: source.title,
            url: source.url,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlowCardSummary {
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
    pub repeat_count: u32,
    pub pinned: bool,
    pub image_count: i64,
    pub pending_count: u32,
    pub thumbnail_urls: Vec<String>,
    pub sources: Vec<CardSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlowCardDetail {
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
    pub repeat_count: u32,
    pub pinned: bool,
    pub image_urls: Vec<String>,
    pub pending_count: u32,
    pub sources: Vec<CardSource>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TipCreated {
    pub id: i64,
    pub topic: String,
    pub tipcard_type: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReviewAndAdvanceOutcome {
    pub next_card: Option<FlowCardSummary>,
    pub daily_complete: bool,
    pub pending_count: u32,
    pub refill_scheduled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyRefreshOutcome {
    CardAvailable,
    QueueRefilled,
    NoChange,
    ActiveLimitReached,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DailyRefreshResult {
    pub refreshed_topics: u64,
    pub available_cards: u64,
    pub generated_cards: u64,
    pub outcome: DailyRefreshOutcome,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsView {
    pub server_version: String,
    pub build_sha: String,
    pub model: String,
    pub vision_model: String,
    pub compress_model: String,
    pub template: String,
    pub api_key: String,
    pub base_url: String,
    pub compress_base_url: String,
    pub reasoning_effort: String,
    pub compress_reasoning_effort: String,
    pub compression_level: String,
    pub color_scheme: String,
    pub transparency: String,
    pub blur_intensity: String,
    pub autoupdate_enabled: bool,
    pub autoupdate_repo: String,
    pub autoupdate_branch: String,
    pub autoupdate_check_interval_secs: u64,
    pub autoupdate_command: String,
    pub autoupdate_last_seen_sha: String,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub max_active_cards: u64,
    pub grounding_strategy: String,
    pub image_strategy: String,
    pub search_provider: String,
    pub scrape_provider: String,
    pub search_api_key: String,
    pub search_base_url: String,
    pub image_sources: String,
    pub grounding_model: String,
    pub grounding_reasoning_effort: String,
}

#[derive(Clone, Debug, Default)]
pub struct SettingsPatch {
    pub vision_model: Option<String>,
    pub model: Option<String>,
    pub compress_model: Option<String>,
    pub template: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub compress_base_url: Option<String>,
    pub reasoning_effort: Option<String>,
    pub compress_reasoning_effort: Option<String>,
    pub compression_level: Option<String>,
    pub autoupdate_enabled: Option<bool>,
    pub autoupdate_repo: Option<String>,
    pub autoupdate_branch: Option<String>,
    pub max_active_cards: Option<u64>,
    pub grounding_strategy: Option<String>,
    pub image_strategy: Option<String>,
    pub search_api_key: Option<String>,
    pub search_provider: Option<String>,
    pub scrape_provider: Option<String>,
    pub search_base_url: Option<String>,
    pub image_sources: Option<String>,
    pub autoupdate_check_interval_secs: Option<u64>,
    pub autoupdate_command: Option<String>,
    pub daily_time_zone: Option<String>,
    pub daily_update_time: Option<String>,
    pub grounding_model: Option<String>,
    pub grounding_reasoning_effort: Option<String>,
    pub color_scheme: Option<String>,
    pub transparency: Option<String>,
    pub blur_intensity: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ApiKeyRow {
    pub id: i64,
    pub client_name: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppSummaryView {
    pub due_cards: i64,
    pub active_cards: i64,
    pub total_cards: i64,
    pub topics: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppTopicView {
    pub id: i64,
    pub name: String,
    pub tipcard_type: String,
    pub icon_id: String,
    pub topic_color: String,
    pub prompt_template: String,
    pub total_cards: i64,
    pub due_cards: i64,
    pub pending_cards: i64,
    pub completed_cards: i64,
    pub daily_card_count: u32,
    pub daily_time_zone: String,
    pub daily_update_time: String,
    pub compression_level: String,
    pub grounding_strategy: String,
    pub image_strategy: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRow {
    pub id: i64,
    pub topic_ids: Vec<i64>,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentDetailView {
    pub id: i64,
    pub source_type: String,
    pub title: String,
    pub url: Option<String>,
    pub content: String,
    pub created_at: String,
    pub topic_ids: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoolImageRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InventoryCard {
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
    pub repeat_count: u32,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisionTestView {
    pub ok: bool,
    pub model: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExploredLinkView {
    pub title: String,
    pub url: String,
}

// ---- operations ----

pub async fn list_flow_cards(
    page_size: u32,
    page_token: Option<String>,
) -> ApiResult<FlowCardPage> {
    let response = call_read(api_request::Op::ListFlowCards(ListFlowCardsRequest {
        page_size,
        page_token: page_token.unwrap_or_default(),
    }))
    .await?;
    match response.result {
        Some(api_response::Result::FlowCardPage(page)) => Ok(FlowCardPage {
            cards: page.cards.into_iter().map(map_flow_summary).collect(),
            next_cursor: if page.next_page_token.is_empty() {
                None
            } else {
                Some(page.next_page_token)
            },
            has_more: page.has_more,
        }),
        _ => unexpected("flow_card_page"),
    }
}

pub async fn get_tipcard(id: i64) -> ApiResult<FlowCardDetail> {
    let response = call_read(api_request::Op::GetTipcard(GetByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::TipcardDetail(detail)) => {
            let card = detail.card.ok_or_else(|| ApiError {
                status: 200,
                message: "Tipcard detail missing card".into(),
                retryable: false,
                mutation_outcome_indeterminate: false,
                request_id: String::new(),
            })?;
            Ok(map_flow_detail(card))
        }
        _ => unexpected("tipcard_detail"),
    }
}

pub async fn tips_v1(
    count: Option<u32>,
    topics: Vec<String>,
    tipcard_type: &str,
    exclude_card_ids: Option<Vec<i64>>,
    manual_content: Option<String>,
    manual_image_data: Option<Vec<String>>,
) -> ApiResult<Vec<TipCreated>> {
    let response = call_mutation(api_request::Op::TipsV1(TipsRequestV1 {
        count: count.unwrap_or(0),
        topics,
        tipcard_type: tipcard_type_value(tipcard_type) as i32,
        exclude_card_ids: exclude_card_ids.unwrap_or_default(),
        manual_content: manual_content.unwrap_or_default(),
        manual_compressed_content: String::new(),
        manual_image_data: manual_image_data.unwrap_or_default(),
    }))
    .await?;
    match response.result {
        Some(api_response::Result::Tips(tips)) => Ok(tips
            .tips
            .into_iter()
            .map(|t| TipCreated {
                id: t.id,
                topic: t.topic,
                tipcard_type: t.tipcard_type,
                pinned: t.pinned,
            })
            .collect()),
        _ => unexpected("tips"),
    }
}

pub async fn review_and_advance_with_key(
    card_id: i64,
    grade: Option<u8>,
    action: Option<String>,
    idempotency_key: String,
) -> ApiResult<ReviewAndAdvanceOutcome> {
    let response = call_mutation_with_key(
        api_request::Op::ReviewAndAdvance(ReviewAndAdvanceRequest {
            card_id,
            grade: grade.unwrap_or(0) as u32,
            action: review_action_value(action.as_deref()) as i32,
        }),
        idempotency_key,
    )
    .await?;
    match response.result {
        Some(api_response::Result::ReviewAndAdvance(result)) => Ok(ReviewAndAdvanceOutcome {
            next_card: result.next_card.map(map_flow_summary),
            daily_complete: result.daily_complete,
            pending_count: result.pending_count,
            refill_scheduled: result.refill_scheduled,
        }),
        _ => unexpected("review_and_advance"),
    }
}

pub async fn continue_daily_review(
    topics: Vec<String>,
    tipcard_type: Option<String>,
) -> ApiResult<u64> {
    let response = call_mutation(api_request::Op::ContinueDailyReview(
        ContinueDailyReviewRequest {
            topics,
            tipcard_type: tipcard_type.unwrap_or_default(),
        },
    ))
    .await?;
    match response.result {
        Some(api_response::Result::ContinueDailyReview(r)) => Ok(r.available_cards),
        _ => unexpected("continue_daily_review"),
    }
}

pub async fn pin_tipcard(id: i64, pinned: bool) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::PinTipcard(PinTipcardRequest {
        id,
        pinned,
    }))
    .await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn delete_tipcard(id: i64) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::DeleteTipcard(DeleteByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn list_tipcards() -> ApiResult<Vec<InventoryCard>> {
    let response = call_read(api_request::Op::ListTipcards(Empty {})).await?;
    match response.result {
        Some(api_response::Result::Tipcards(list)) => Ok(list
            .cards
            .into_iter()
            .map(|c| InventoryCard {
                id: c.id,
                topic_name: c.topic_name,
                topic_icon: c.topic_icon,
                topic_color: c.topic_color,
                title: c.title,
                full_content: c.full_content,
                compressed_content: c.compressed_content,
                created_at: c.created_at,
                tipcard_type: c.tipcard_type,
                status: c.status,
                next_review_at: c.next_review_at,
                repeat_count: c.repeat_count,
                pinned: c.pinned,
            })
            .collect()),
        _ => unexpected("tipcards"),
    }
}

pub async fn force_daily_refresh(
    topics: String,
    tipcard_type: Option<String>,
) -> ApiResult<DailyRefreshResult> {
    let response = call_mutation(api_request::Op::ForceDailyRefresh(
        ForceDailyRefreshRequest {
            topics,
            tipcard_type: tipcard_type.unwrap_or_default(),
        },
    ))
    .await?;
    match response.result {
        Some(api_response::Result::ForceDailyRefresh(r)) => {
            let outcome = match ForceDailyRefreshOutcome::try_from(r.outcome).ok() {
                Some(ForceDailyRefreshOutcome::CardAvailable) => DailyRefreshOutcome::CardAvailable,
                Some(ForceDailyRefreshOutcome::QueueRefilled) => DailyRefreshOutcome::QueueRefilled,
                Some(ForceDailyRefreshOutcome::ActiveLimitReached) => {
                    DailyRefreshOutcome::ActiveLimitReached
                }
                Some(
                    ForceDailyRefreshOutcome::NoChange | ForceDailyRefreshOutcome::Unspecified,
                )
                | None => DailyRefreshOutcome::NoChange,
            };
            Ok(DailyRefreshResult {
                refreshed_topics: r.refreshed_cards,
                available_cards: r.available_cards,
                generated_cards: r.generated_cards,
                outcome,
            })
        }
        _ => unexpected("force_daily_refresh"),
    }
}

pub async fn get_settings() -> ApiResult<SettingsView> {
    let response = call_read(api_request::Op::GetSettings(Empty {})).await?;
    let settings = match response.result {
        Some(api_response::Result::Settings(s)) => s,
        _ => return unexpected("settings"),
    };
    // Optional version enrichment for settings UI parity with dashboard.
    let (server_version, build_sha) = match call_read(api_request::Op::GetApiInfo(Empty {})).await {
        Ok(info_resp) => match info_resp.result {
            Some(api_response::Result::ApiInfo(info)) => (info.server_version, info.build_sha),
            _ => (String::new(), String::new()),
        },
        Err(_) => (String::new(), String::new()),
    };
    Ok(SettingsView {
        server_version,
        build_sha,
        model: settings.model,
        vision_model: settings.vision_model,
        compress_model: settings.compress_model,
        template: settings.template,
        api_key: settings.api_key,
        base_url: settings.base_url,
        compress_base_url: settings.compress_base_url,
        reasoning_effort: settings.reasoning_effort,
        compress_reasoning_effort: settings.compress_reasoning_effort,
        compression_level: settings.compression_level,
        color_scheme: settings.color_scheme,
        transparency: settings.transparency,
        blur_intensity: settings.blur_intensity,
        autoupdate_enabled: settings.autoupdate_enabled,
        autoupdate_repo: settings.autoupdate_repo,
        autoupdate_branch: settings.autoupdate_branch,
        autoupdate_check_interval_secs: settings.autoupdate_check_interval_secs,
        autoupdate_command: settings.autoupdate_command,
        autoupdate_last_seen_sha: settings.autoupdate_last_seen_sha,
        daily_time_zone: settings.daily_time_zone,
        daily_update_time: settings.daily_update_time,
        max_active_cards: settings.max_active_cards,
        grounding_strategy: settings.grounding_strategy,
        image_strategy: settings.image_strategy,
        search_provider: settings.search_provider,
        scrape_provider: settings.scrape_provider,
        search_api_key: settings.search_api_key,
        search_base_url: settings.search_base_url,
        image_sources: settings.image_sources,
        grounding_model: settings.grounding_model,
        grounding_reasoning_effort: settings.grounding_reasoning_effort,
    })
}

pub async fn update_settings(patch: SettingsPatch) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::UpdateSettings(UpdateSettingsRequest {
        model: patch.model,
        compress_model: patch.compress_model,
        template: patch.template,
        api_key: patch.api_key,
        base_url: patch.base_url,
        compress_base_url: patch.compress_base_url,
        reasoning_effort: patch.reasoning_effort,
        compress_reasoning_effort: patch.compress_reasoning_effort,
        color_scheme: patch.color_scheme,
        autoupdate_enabled: patch.autoupdate_enabled,
        autoupdate_repo: patch.autoupdate_repo,
        autoupdate_branch: patch.autoupdate_branch,
        autoupdate_check_interval_secs: patch.autoupdate_check_interval_secs,
        autoupdate_command: patch.autoupdate_command,
        daily_time_zone: patch.daily_time_zone,
        daily_update_time: patch.daily_update_time,
        max_active_cards: patch.max_active_cards,
        compression_level: patch.compression_level,
        grounding_strategy: patch.grounding_strategy,
        image_strategy: patch.image_strategy,
        search_api_key: patch.search_api_key,
        search_base_url: patch.search_base_url,
        image_sources: patch.image_sources,
        grounding_model: patch.grounding_model,
        grounding_reasoning_effort: patch.grounding_reasoning_effort,
        search_provider: patch.search_provider,
        scrape_provider: patch.scrape_provider,
        vision_model: patch.vision_model,
        transparency: patch.transparency,
        blur_intensity: patch.blur_intensity,
    }))
    .await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn test_vision_model() -> ApiResult<VisionTestView> {
    let response = call_read(api_request::Op::TestVisionModel(Empty {})).await?;
    match response.result {
        Some(api_response::Result::VisionModelTest(t)) => Ok(VisionTestView {
            ok: t.ok,
            model: t.model,
            message: t.message,
        }),
        _ => unexpected("vision_model_test"),
    }
}

pub async fn list_api_keys() -> ApiResult<Vec<ApiKeyRow>> {
    let response = call_read(api_request::Op::ListApiKeys(Empty {})).await?;
    match response.result {
        Some(api_response::Result::ApiKeys(keys)) => Ok(keys
            .keys
            .into_iter()
            .map(|k| ApiKeyRow {
                id: k.id,
                client_name: k.client_name,
                created_at: k.created_at,
            })
            .collect()),
        _ => unexpected("api_keys"),
    }
}

pub async fn create_api_key(client_name: Option<String>) -> ApiResult<String> {
    let response = call_mutation(api_request::Op::CreateApiKey(CreateApiKeyRequest {
        client_name: client_name.unwrap_or_default(),
    }))
    .await?;
    match response.result {
        Some(api_response::Result::ApiKeyCreated(created)) => Ok(created.api_key),
        _ => unexpected("api_key_created"),
    }
}

pub async fn delete_api_key(id: i64) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::DeleteApiKey(DeleteByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn get_summary() -> ApiResult<AppSummaryView> {
    let response = call_read(api_request::Op::GetSummary(Empty {})).await?;
    match response.result {
        Some(api_response::Result::Summary(s)) => Ok(AppSummaryView {
            due_cards: s.due_cards,
            active_cards: s.active_cards,
            total_cards: s.total_cards,
            topics: s.topics,
        }),
        _ => unexpected("summary"),
    }
}

pub async fn list_app_topics() -> ApiResult<Vec<AppTopicView>> {
    let response = call_read(api_request::Op::ListAppTopics(Empty {})).await?;
    match response.result {
        Some(api_response::Result::AppTopics(topics)) => Ok(topics
            .topics
            .into_iter()
            .map(|t| AppTopicView {
                id: t.id,
                name: t.name,
                tipcard_type: t.tipcard_type,
                icon_id: t.icon_id,
                topic_color: t.topic_color,
                prompt_template: t.prompt_template,
                total_cards: t.total_cards,
                due_cards: t.due_cards,
                pending_cards: t.pending_cards,
                completed_cards: t.completed_cards,
                daily_card_count: t.daily_card_count,
                daily_time_zone: t.daily_time_zone,
                daily_update_time: t.daily_update_time,
                compression_level: t.compression_level,
                grounding_strategy: t.grounding_strategy,
                image_strategy: t.image_strategy,
            })
            .collect()),
        _ => unexpected("app_topics"),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn update_topic(
    id: i64,
    prompt_template: Option<String>,
    daily_card_count: Option<u32>,
    daily_time_zone: Option<String>,
    daily_update_time: Option<String>,
    compression_level: Option<String>,
    grounding_strategy: Option<String>,
    image_strategy: Option<String>,
) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::UpdateTopic(UpdateTopicRequest {
        id,
        prompt_template,
        daily_card_count,
        daily_time_zone,
        daily_update_time,
        compression_level,
        grounding_strategy,
        image_strategy,
    }))
    .await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn delete_topic(id: i64) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::DeleteTopic(DeleteByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn list_documents() -> ApiResult<Vec<DocumentRow>> {
    let response = call_read(api_request::Op::ListDocuments(Empty {})).await?;
    match response.result {
        Some(api_response::Result::Documents(docs)) => Ok(docs
            .docs
            .into_iter()
            .map(|d| DocumentRow {
                id: d.id,
                topic_ids: d.topic_ids,
                source_type: d.source_type,
                title: d.title,
                url: empty_opt(d.url),
                created_at: d.created_at,
            })
            .collect()),
        _ => unexpected("documents"),
    }
}

pub async fn get_document(id: i64) -> ApiResult<DocumentDetailView> {
    let response = call_read(api_request::Op::GetDocument(GetByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::DocumentDetail(d)) => Ok(DocumentDetailView {
            id: d.id,
            source_type: d.source_type,
            title: d.title,
            url: empty_opt(d.url),
            content: d.content,
            created_at: d.created_at,
            topic_ids: d.topic_ids,
        }),
        _ => unexpected("document_detail"),
    }
}

pub async fn create_document(
    topic_ids: Vec<i64>,
    source_type: String,
    title: String,
    url: Option<String>,
    content: String,
) -> ApiResult<DocumentDetailView> {
    let response = call_mutation(api_request::Op::CreateDocument(AddDocumentRequest {
        topic_id_opt: String::new(),
        source_type,
        title,
        url: url.unwrap_or_default(),
        content,
        topic_ids,
    }))
    .await?;
    match response.result {
        Some(api_response::Result::DocumentCreated(d))
        | Some(api_response::Result::DocumentDetail(d)) => Ok(DocumentDetailView {
            id: d.id,
            source_type: d.source_type,
            title: d.title,
            url: empty_opt(d.url),
            content: d.content,
            created_at: d.created_at,
            topic_ids: d.topic_ids,
        }),
        _ => unexpected("document_created"),
    }
}

pub async fn upload_document(
    topic_ids: Vec<i64>,
    filename: String,
    mime_type: String,
    title: Option<String>,
    data: Vec<u8>,
) -> ApiResult<DocumentDetailView> {
    let response = call_mutation(api_request::Op::UploadDocument(UploadDocumentRequest {
        topic_ids,
        filename,
        mime_type,
        title: title.unwrap_or_default(),
        data,
    }))
    .await?;
    match response.result {
        Some(api_response::Result::DocumentCreated(d))
        | Some(api_response::Result::DocumentDetail(d)) => Ok(DocumentDetailView {
            id: d.id,
            source_type: d.source_type,
            title: d.title,
            url: empty_opt(d.url),
            content: d.content,
            created_at: d.created_at,
            topic_ids: d.topic_ids,
        }),
        _ => unexpected("document_created"),
    }
}

/// Decode a `data:` URL (or raw base64) into bytes for `upload_document`.
pub fn decode_data_url(data_url: &str) -> ApiResult<(String, Vec<u8>)> {
    let (meta, b64) = if let Some(rest) = data_url.strip_prefix("data:") {
        match rest.split_once(',') {
            Some((meta, b64)) => (meta.to_string(), b64),
            None => {
                return Err(ApiError {
                    status: 0,
                    message: "Invalid data URL".into(),
                    retryable: false,
                    mutation_outcome_indeterminate: false,
                    request_id: String::new(),
                });
            }
        }
    } else {
        ("application/octet-stream;base64".into(), data_url)
    };
    let mime = meta
        .split(';')
        .next()
        .unwrap_or("application/octet-stream")
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|err| ApiError {
            status: 0,
            message: format!("Invalid base64 payload: {err}"),
            retryable: false,
            mutation_outcome_indeterminate: false,
            request_id: String::new(),
        })?;
    Ok((mime, bytes))
}

pub async fn delete_document(id: i64) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::DeleteDocument(DeleteByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn attach_document_topic(document_id: i64, topic_id: i64) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::AttachDocumentTopic(
        AttachDocumentTopicRequest {
            document_id,
            topic_id,
        },
    ))
    .await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn detach_document_topic(document_id: i64, topic_id: i64) -> ApiResult<()> {
    let response = call_mutation(api_request::Op::DetachDocumentTopic(
        AttachDocumentTopicRequest {
            document_id,
            topic_id,
        },
    ))
    .await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub async fn explore_link(url: String) -> ApiResult<Vec<ExploredLinkView>> {
    let response = call_read(api_request::Op::ExploreLink(ExploreLinkRequest { url })).await?;
    match response.result {
        Some(api_response::Result::ExploredLinks(links)) => Ok(links
            .links
            .into_iter()
            .map(|l| ExploredLinkView {
                title: l.title,
                url: l.url,
            })
            .collect()),
        _ => unexpected("explored_links"),
    }
}

pub async fn list_pool_images() -> ApiResult<Vec<PoolImageRow>> {
    let response = call_read(api_request::Op::ListPoolImages(Empty {})).await?;
    match response.result {
        Some(api_response::Result::PoolImages(images)) => Ok(images
            .images
            .into_iter()
            .map(|img| PoolImageRow {
                id: img.id,
                name: img.name,
                description: empty_opt(img.description),
                tags: img.tags,
                created_at: img.created_at,
            })
            .collect()),
        _ => unexpected("pool_images"),
    }
}

pub async fn create_pool_image(
    image_data: String,
    name: String,
    description: Option<String>,
) -> ApiResult<i64> {
    let response = call_mutation(api_request::Op::CreatePoolImage(AddPoolImageRequest {
        image_data,
        name,
        description: description.unwrap_or_default(),
    }))
    .await?;
    match response.result {
        Some(api_response::Result::PoolImageCreated(created)) => Ok(created.id),
        _ => unexpected("pool_image_created"),
    }
}

pub async fn delete_pool_image(id: i64) -> ApiResult<()> {
    let response =
        call_mutation(api_request::Op::DeletePoolImage(DeleteByIdRequest { id })).await?;
    match response.result {
        Some(api_response::Result::Ok(_)) => Ok(()),
        _ => unexpected("ok"),
    }
}

pub fn pool_image_url(id: i64) -> String {
    format!("/api/v1/pool-images/{id}")
}

pub fn tipcard_image_url(id: i64) -> String {
    format!("/api/v1/tipcard-images/{id}")
}

// ---- helpers ----

fn map_flow_summary(card: pb::FlowCardInfo) -> FlowCardSummary {
    let thumbnail_urls = card
        .images
        .iter()
        .map(|img| {
            if img.download_path.is_empty() {
                tipcard_image_url(img.id)
            } else {
                img.download_path.clone()
            }
        })
        .collect::<Vec<_>>();
    let image_count = card.images.len() as i64;
    FlowCardSummary {
        id: card.id,
        topic_name: card.topic_name,
        topic_icon: card.topic_icon,
        topic_color: card.topic_color,
        title: card.title,
        full_content: card.full_content,
        compressed_content: card.compressed_content,
        created_at: card.created_at,
        tipcard_type: card.tipcard_type,
        status: card.status,
        next_review_at: card.next_review_at,
        repeat_count: card.repeat_count,
        pinned: card.pinned,
        image_count,
        pending_count: card.pending_count as u32,
        thumbnail_urls,
        sources: card.sources.into_iter().map(Into::into).collect(),
    }
}

fn map_flow_detail(card: pb::FlowCardInfo) -> FlowCardDetail {
    let image_urls = card
        .images
        .iter()
        .map(|img| {
            if img.download_path.is_empty() {
                tipcard_image_url(img.id)
            } else {
                img.download_path.clone()
            }
        })
        .collect();
    FlowCardDetail {
        id: card.id,
        topic_name: card.topic_name,
        topic_icon: card.topic_icon,
        topic_color: card.topic_color,
        title: card.title,
        full_content: card.full_content,
        compressed_content: card.compressed_content,
        created_at: card.created_at,
        tipcard_type: card.tipcard_type,
        status: card.status,
        next_review_at: card.next_review_at,
        repeat_count: card.repeat_count,
        pinned: card.pinned,
        image_urls,
        pending_count: card.pending_count as u32,
        sources: card.sources.into_iter().map(Into::into).collect(),
    }
}

fn tipcard_type_value(raw: &str) -> TipcardTypeValue {
    match raw {
        "repeatable_tip" => TipcardTypeValue::Repeatable,
        "casual_tip" => TipcardTypeValue::Casual,
        "manual_tip" => TipcardTypeValue::Manual,
        "custom_tip" => TipcardTypeValue::Custom,
        _ => TipcardTypeValue::Unspecified,
    }
}

/// Map UI/dashboard action strings onto the typed v1 enum.
///
/// - Empty / missing → `Unspecified` (grade-only; server applies empty action `""`)
/// - `acknowledge` → casual/manual Acknowledge
/// - `dismiss` is a legacy alias of `skip_not_interested` (domain maps both the same)
fn review_action_value(raw: Option<&str>) -> ReviewActionValue {
    match raw.map(str::trim).unwrap_or("") {
        "" => ReviewActionValue::Unspecified,
        "again" | "repeat" => ReviewActionValue::Again,
        "learned" | "memorize" => ReviewActionValue::Learned,
        "skip_known" => ReviewActionValue::SkipKnown,
        // Domain: "skip_not_interested" | "dismiss" → not_interested feedback
        "skip_not_interested" | "dismiss" => ReviewActionValue::SkipNotInterested,
        "skip_too_difficult" => ReviewActionValue::SkipTooDifficult,
        "acknowledge" | "acknowledged" => ReviewActionValue::Acknowledge,
        // Unknown names fall through as grade-only rather than inventing an action.
        _ => ReviewActionValue::Unspecified,
    }
}

fn empty_opt(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

fn unexpected<T>(expected: &str) -> ApiResult<T> {
    Err(ApiError {
        status: 200,
        message: format!("Unexpected API result (expected {expected})"),
        retryable: false,
        mutation_outcome_indeterminate: false,
        request_id: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pb::ReviewRequestV1;

    #[test]
    fn tipcard_type_mapping() {
        assert_eq!(
            tipcard_type_value("repeatable_tip") as i32,
            TipcardTypeValue::Repeatable as i32
        );
        assert_eq!(
            tipcard_type_value("manual_tip") as i32,
            TipcardTypeValue::Manual as i32
        );
    }

    #[test]
    fn review_action_mapping_includes_legacy_aliases() {
        assert_eq!(
            review_action_value(Some("again")) as i32,
            ReviewActionValue::Again as i32
        );
        assert_eq!(
            review_action_value(Some("repeat")) as i32,
            ReviewActionValue::Again as i32
        );
        assert_eq!(
            review_action_value(Some("memorize")) as i32,
            ReviewActionValue::Learned as i32
        );
    }

    #[test]
    fn review_action_maps_casual_acknowledge() {
        // flow_card casual/manual primary button emits "acknowledge"
        assert_eq!(
            review_action_value(Some("acknowledge")) as i32,
            ReviewActionValue::Acknowledge as i32
        );
        assert_eq!(
            review_action_value(Some("acknowledged")) as i32,
            ReviewActionValue::Acknowledge as i32
        );
    }

    #[test]
    fn review_action_maps_empty_grade_only() {
        // flow_card Again/Good/Easy emit Some("") for grade-only reviews
        assert_eq!(
            review_action_value(Some("")) as i32,
            ReviewActionValue::Unspecified as i32
        );
        assert_eq!(
            review_action_value(Some("   ")) as i32,
            ReviewActionValue::Unspecified as i32
        );
        assert_eq!(
            review_action_value(None) as i32,
            ReviewActionValue::Unspecified as i32
        );
    }

    #[test]
    fn review_action_maps_dismiss_to_not_interested() {
        // Domain treats dismiss as skip_not_interested (not skip_known)
        assert_eq!(
            review_action_value(Some("dismiss")) as i32,
            ReviewActionValue::SkipNotInterested as i32
        );
        assert_eq!(
            review_action_value(Some("skip_not_interested")) as i32,
            ReviewActionValue::SkipNotInterested as i32
        );
        assert_eq!(
            review_action_value(Some("skip_known")) as i32,
            ReviewActionValue::SkipKnown as i32
        );
    }

    #[test]
    fn review_action_maps_repeatable_named_actions() {
        assert_eq!(
            review_action_value(Some("learned")) as i32,
            ReviewActionValue::Learned as i32
        );
        assert_eq!(
            review_action_value(Some("skip_too_difficult")) as i32,
            ReviewActionValue::SkipTooDifficult as i32
        );
    }

    #[test]
    fn review_v1_request_encodes_acknowledge_and_grade_only() {
        // Drive the real request-building path used by the shipped client.
        let ack = ReviewRequestV1 {
            card_id: 42,
            grade: 3,
            action: review_action_value(Some("acknowledge")) as i32,
        };
        assert_eq!(ack.action, ReviewActionValue::Acknowledge as i32);

        let grade_only = ReviewRequestV1 {
            card_id: 7,
            grade: 5,
            action: review_action_value(Some("")) as i32,
        };
        assert_eq!(grade_only.action, ReviewActionValue::Unspecified as i32);

        let dismiss = ReviewRequestV1 {
            card_id: 9,
            grade: 3,
            action: review_action_value(Some("dismiss")) as i32,
        };
        assert_eq!(dismiss.action, ReviewActionValue::SkipNotInterested as i32);
    }

    #[test]
    fn decode_data_url_plain_base64() {
        let raw = base64::engine::general_purpose::STANDARD.encode(b"hello");
        let (mime, bytes) = decode_data_url(&raw).unwrap();
        assert_eq!(mime, "application/octet-stream");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn decode_data_url_with_prefix() {
        let raw = format!(
            "data:text/plain;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b"abc")
        );
        let (mime, bytes) = decode_data_url(&raw).unwrap();
        assert_eq!(mime, "text/plain");
        assert_eq!(bytes, b"abc");
    }

    #[test]
    fn map_flow_summary_uses_download_paths() {
        let card = pb::FlowCardInfo {
            id: 9,
            topic_name: "t".into(),
            topic_icon: "icon".into(),
            topic_color: "#fff".into(),
            title: "Title".into(),
            full_content: "full".into(),
            compressed_content: "c".into(),
            created_at: "now".into(),
            tipcard_type: "repeatable_tip".into(),
            status: "active".into(),
            next_review_at: String::new(),
            repeat_count: 0,
            pinned: false,
            pending_count: 3,
            images: vec![pb::TipcardImageInfo {
                id: 42,
                position: 0,
                mime_type: "image/png".into(),
                byte_size: 10,
                download_path: "/api/v1/tipcard-images/42".into(),
            }],
            sources: vec![pb::CardSource {
                document_id: 7,
                source_type: "link".into(),
                title: "Rust book".into(),
                url: Some("https://doc.rust-lang.org/book".into()),
            }],
        };
        let mapped = map_flow_summary(card);
        assert_eq!(mapped.image_count, 1);
        assert_eq!(mapped.pending_count, 3);
        assert_eq!(mapped.thumbnail_urls, vec!["/api/v1/tipcard-images/42"]);
        assert_eq!(mapped.sources.len(), 1);
        assert_eq!(mapped.sources[0].title, "Rust book");
        assert_eq!(
            mapped.sources[0].url.as_deref(),
            Some("https://doc.rust-lang.org/book")
        );
    }
}
