use crate::api::toast;
use crate::app::View;
use crate::components::flow_card::{FlowCard, FlowCardSkeleton};
use crate::i18n::use_i18n;
use crate::image_compress::{collect_files, compress_files_to_data_urls};
use crate::state::AppState;
use gloo_net::http::Request;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsCast;
use web_sys::{DragEvent, HtmlInputElement, HtmlTextAreaElement, KeyboardEvent};
use yew::prelude::*;
use yew_router::prelude::*;

const PAGE_LIMIT: i64 = 48;
const TRANSMISSION_MAX_PICKS: usize = 9;
const TRANSMISSION_MAX_PICKS_PER_TOPIC: usize = 3;
const REVIEWED_PLACEHOLDERS_KEY: &str = "denpie-reviewed-placeholders";
const DRAG_SCROLL_EDGE_PX: f64 = 96.0;
const DRAG_SCROLL_MAX_STEP_PX: f64 = 32.0;

#[derive(Deserialize, Serialize, Clone, PartialEq)]
pub struct TipcardInfo {
    pub id: i64,
    pub topic_name: String,
    pub topic_icon: String,
    pub topic_color: String,
    pub title: String,
    pub full_content: String,
    pub compressed_content: String,
    pub image_data: Vec<String>,
    pub created_at: String,
    pub tipcard_type: String,
    pub status: String,
    pub next_review_at: String,
    pub repeat_count: u32,
    pub pinned: bool,
    #[serde(default)]
    pub pending_count: u32,
    /// Client-only placeholder text after a review action; API responses omit this.
    #[serde(default)]
    pub review_message: Option<String>,
}

#[derive(Deserialize, Clone, PartialEq)]
struct FlowCardSummary {
    id: i64,
    topic_name: String,
    topic_icon: String,
    topic_color: String,
    title: String,
    full_content: String,
    compressed_content: String,
    created_at: String,
    tipcard_type: String,
    status: String,
    next_review_at: String,
    repeat_count: u32,
    pinned: bool,
    image_count: i64,
    #[serde(default)]
    pending_count: u32,
    thumbnail_urls: Vec<String>,
}

#[derive(Deserialize)]
struct FlowCardPage {
    cards: Vec<FlowCardSummary>,
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Deserialize, Clone)]
struct FlowCardDetail {
    id: i64,
    topic_name: String,
    topic_icon: String,
    topic_color: String,
    title: String,
    full_content: String,
    compressed_content: String,
    created_at: String,
    tipcard_type: String,
    status: String,
    next_review_at: String,
    repeat_count: u32,
    pinned: bool,
    image_urls: Vec<String>,
}

impl From<FlowCardSummary> for TipcardInfo {
    fn from(card: FlowCardSummary) -> Self {
        Self {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.full_content,
            compressed_content: card.compressed_content,
            image_data: card.thumbnail_urls,
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeat_count,
            pinned: card.pinned,
            pending_count: card.pending_count,
            review_message: None,
        }
    }
}

impl From<FlowCardDetail> for TipcardInfo {
    fn from(card: FlowCardDetail) -> Self {
        Self {
            id: card.id,
            topic_name: card.topic_name,
            topic_icon: card.topic_icon,
            topic_color: card.topic_color,
            title: card.title,
            full_content: card.full_content,
            compressed_content: card.compressed_content,
            image_data: card.image_urls,
            created_at: card.created_at,
            tipcard_type: card.tipcard_type,
            status: card.status,
            next_review_at: card.next_review_at,
            repeat_count: card.repeat_count,
            pinned: card.pinned,
            pending_count: 0,
            review_message: None,
        }
    }
}

#[derive(Serialize)]
struct CreateTipReq {
    count: Option<u32>,
    topics: String,
    tipcard_type: Option<String>,
    manual_content: Option<String>,
    manual_image_data: Option<Vec<String>>,
    exclude_card_ids: Option<Vec<i64>>,
}

#[derive(Serialize)]
struct ReviewReq {
    card_id: i64,
    grade: Option<u8>,
    action: Option<String>,
}

#[derive(Serialize)]
struct PinReq {
    id: i64,
    pinned: Option<bool>,
    image_data: Option<Vec<String>>,
}

#[function_component(UnifiedFlow)]
pub fn unified_flow() -> Html {
    let app_state = use_context::<UseReducerHandle<AppState>>().unwrap();
    let i18n = use_i18n();
    let cards = use_state(Vec::<TipcardInfo>::new);
    let reviewed_placeholders = use_state(|| {
        LocalStorage::get::<Vec<TipcardInfo>>(REVIEWED_PLACEHOLDERS_KEY)
            .unwrap_or_default()
            .into_iter()
            .map(|card| (card.id, card))
            .collect::<HashMap<_, _>>()
    });
    let detail_loaded = use_state(HashMap::<i64, bool>::new);
    let card_heights = use_state(HashMap::<i64, f64>::new);
    let next_cursor = use_state(|| None::<String>);
    let has_more = use_state(|| true);
    let loading = use_state(|| false);
    let pending_count = use_state(|| 0usize);
    let card_order =
        use_state(|| LocalStorage::get::<Vec<i64>>("denpie-card-order").unwrap_or_default());
    let pinned_card_order =
        use_state(|| LocalStorage::get::<Vec<i64>>("denpie-pinned-card-order").unwrap_or_default());
    let topics_input =
        use_state(|| LocalStorage::get::<String>("denpie_prefill_topic").unwrap_or_default());
    let tip_type = use_state(|| {
        LocalStorage::get::<String>("denpie_prefill_type")
            .unwrap_or_else(|_| "casual_tip".to_string())
    });
    let manual_content = use_state(String::new);
    let manual_images = use_state(Vec::<String>::new);
    let layout = use_state(|| {
        LocalStorage::get::<String>("denpie-flow-layout").unwrap_or_else(|_| "grid".to_string())
    });
    let sort_by = use_state(|| {
        LocalStorage::get::<String>("denpie-flow-sort")
            .map(|value| normalize_flow_sort(&value))
            .unwrap_or_else(|_| "topic".to_string())
    });
    let fullscreen_card_key = use_state(|| None::<String>);

    {
        let placeholders = (*reviewed_placeholders).clone();
        use_effect_with(placeholders, move |placeholders| {
            if placeholders.is_empty() {
                LocalStorage::delete(REVIEWED_PLACEHOLDERS_KEY);
            } else {
                let cards = placeholders.values().cloned().collect::<Vec<_>>();
                let _ = LocalStorage::set(REVIEWED_PLACEHOLDERS_KEY, cards);
            }
            || ()
        });
    }

    let load_cards = {
        let cards = cards.clone();
        let reviewed_placeholders = reviewed_placeholders.clone();
        let detail_loaded = detail_loaded.clone();
        let next_cursor = next_cursor.clone();
        let has_more = has_more.clone();
        let loading = loading.clone();
        Callback::from(move |reset: bool| {
            if *loading {
                return;
            }
            let cards = cards.clone();
            let reviewed_placeholders = reviewed_placeholders.clone();
            let detail_loaded = detail_loaded.clone();
            let next_cursor = next_cursor.clone();
            let has_more = has_more.clone();
            let loading = loading.clone();
            let cursor = if reset { None } else { (*next_cursor).clone() };
            if !reset && cursor.is_none() && !*has_more {
                return;
            }
            loading.set(true);
            wasm_bindgen_futures::spawn_local(async move {
                let mut url = format!("/app/flow-cards?limit={PAGE_LIMIT}");
                if let Some(cursor) = cursor {
                    url.push_str("&cursor=");
                    url.push_str(
                        &js_sys::encode_uri_component(&cursor)
                            .as_string()
                            .unwrap_or_default(),
                    );
                }
                match Request::get(&url).send().await {
                    Ok(res) if res.ok() => {
                        if let Ok(page) = res.json::<FlowCardPage>().await {
                            let mut new_cards: Vec<TipcardInfo> =
                                page.cards.into_iter().map(Into::into).collect();
                            if reset {
                                let loaded_ids =
                                    new_cards.iter().map(|card| card.id).collect::<HashSet<_>>();
                                let active_repeatable_topics = new_cards
                                    .iter()
                                    .filter(|card| {
                                        card.tipcard_type == "repeatable_tip"
                                            && card.status == "active"
                                    })
                                    .map(|card| card.topic_name.as_str())
                                    .collect::<HashSet<_>>();
                                let mut placeholder_map = (*reviewed_placeholders).clone();
                                placeholder_map.retain(|id, card| {
                                    !loaded_ids.contains(id)
                                        && !active_repeatable_topics
                                            .contains(card.topic_name.as_str())
                                });
                                let placeholders =
                                    placeholder_map.values().cloned().collect::<Vec<_>>();
                                reviewed_placeholders.set(placeholder_map);
                                new_cards.extend(placeholders);
                                let loaded = new_cards
                                    .iter()
                                    .map(|card| (card.id, false))
                                    .collect::<HashMap<_, _>>();
                                detail_loaded.set(loaded);
                                cards.set(new_cards);
                            } else {
                                let mut merged = (*cards).clone();
                                let mut loaded = (*detail_loaded).clone();
                                let mut placeholders = (*reviewed_placeholders).clone();
                                for card in new_cards {
                                    if let Some(existing) =
                                        merged.iter_mut().find(|existing| existing.id == card.id)
                                    {
                                        if existing.review_message.is_some() {
                                            placeholders.remove(&card.id);
                                            loaded.insert(card.id, false);
                                            *existing = card;
                                        }
                                    } else {
                                        loaded.entry(card.id).or_insert(false);
                                        merged.push(card);
                                    }
                                }
                                reviewed_placeholders.set(placeholders);
                                detail_loaded.set(loaded);
                                cards.set(merged);
                            }
                            next_cursor.set(page.next_cursor);
                            has_more.set(page.has_more);
                        }
                    }
                    _ => {}
                }
                loading.set(false);
            });
        })
    };

    {
        let load_cards = load_cards.clone();
        let refresh = Callback::from(move |_| load_cards.emit(true));
        crate::hooks::use_view_refresh(crate::app::View::Flow, refresh);
    }

    let request_detail = {
        let cards = cards.clone();
        let detail_loaded = detail_loaded.clone();
        Callback::from(move |id: i64| {
            let cards = cards.clone();
            let detail_loaded = detail_loaded.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(res) = Request::get(&format!("/app/flow-cards/{id}")).send().await {
                    if res.ok() {
                        if let Ok(detail) = res.json::<FlowCardDetail>().await {
                            let mut updated_card: TipcardInfo = detail.into();
                            let mut next = (*cards).clone();
                            if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                                updated_card.pending_count = card.pending_count;
                                *card = updated_card;
                            }
                            let mut loaded = (*detail_loaded).clone();
                            loaded.insert(id, true);
                            detail_loaded.set(loaded);
                            cards.set(next);
                        }
                    }
                }
            });
        })
    };

    let on_submit = {
        let app_state = app_state.clone();
        let topics_input = topics_input.clone();
        let tip_type = tip_type.clone();
        let manual_content = manual_content.clone();
        let manual_images = manual_images.clone();
        let load_cards = load_cards.clone();
        let pending_count = pending_count.clone();

        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();
            if *pending_count > 0 {
                return;
            }
            let app_state = app_state.clone();
            let topics = (*topics_input).clone();
            let ttype = (*tip_type).clone();
            let content = (*manual_content).clone();
            let images = (*manual_images).clone();
            let load_cards = load_cards.clone();
            let pending_count = pending_count.clone();

            let n_skeletons = if ttype == "manual_tip" {
                1
            } else {
                topics
                    .split(',')
                    .filter(|topic| !topic.trim().is_empty())
                    .count()
                    .max(1)
            };
            pending_count.set(n_skeletons);

            wasm_bindgen_futures::spawn_local(async move {
                let req = CreateTipReq {
                    count: (ttype == "repeatable_tip").then_some(5),
                    topics,
                    tipcard_type: Some(ttype.clone()),
                    manual_content: if ttype == "manual_tip" {
                        Some(content)
                    } else {
                        None
                    },
                    manual_image_data: if ttype == "manual_tip" {
                        Some(images)
                    } else {
                        None
                    },
                    exclude_card_ids: None,
                };
                match Request::post("/app/tips").json(&req).unwrap().send().await {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Cards added");
                        LocalStorage::delete("denpie_prefill_topic");
                        LocalStorage::delete("denpie_prefill_type");
                        load_cards.emit(true);
                    }
                    _ => toast(&app_state, "Failed to add cards"),
                }
                pending_count.set(0);
            });
        })
    };

    let on_review_cb = {
        let cards = cards.clone();
        let reviewed_placeholders = reviewed_placeholders.clone();
        let app_state = app_state.clone();
        let load_cards = load_cards.clone();
        Callback::from(
            move |(id, grade, action): (i64, Option<u8>, Option<String>)| {
                let cards = cards.clone();
                let reviewed_placeholders = reviewed_placeholders.clone();
                let app_state = app_state.clone();
                let load_cards = load_cards.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let reviewed_card = cards.iter().find(|card| card.id == id).cloned();
                    let action_name = action.clone().unwrap_or_default();
                    let req = ReviewReq {
                        card_id: id,
                        grade,
                        action,
                    };
                    match Request::post("/app/review")
                        .json(&req)
                        .unwrap()
                        .send()
                        .await
                    {
                        Ok(res) if res.ok() => {
                            if let Some(mut placeholder) = reviewed_card {
                                if placeholder.tipcard_type == "repeatable_tip" {
                                    placeholder.status = "reviewed".to_string();
                                    placeholder.review_message = Some(review_placeholder_message(
                                        &action_name,
                                        &placeholder.next_review_at,
                                    ));
                                    let mut placeholders = (*reviewed_placeholders).clone();
                                    placeholders.retain(|_, card| {
                                        card.topic_name != placeholder.topic_name
                                    });
                                    placeholders.insert(id, placeholder.clone());
                                    reviewed_placeholders.set(placeholders);

                                    let mut next = (*cards).clone();
                                    next.retain(|card| {
                                        !(card.status == "reviewed"
                                            && card.topic_name == placeholder.topic_name)
                                    });
                                    if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                                        *card = placeholder.clone();
                                    } else {
                                        next.push(placeholder.clone());
                                    }
                                    cards.set(next);

                                    let next_req = CreateTipReq {
                                        count: Some(1),
                                        topics: placeholder.topic_name,
                                        tipcard_type: Some("repeatable_tip".to_string()),
                                        manual_content: None,
                                        manual_image_data: None,
                                        exclude_card_ids: Some(vec![id]),
                                    };
                                    let next_ready = match Request::post("/app/tips")
                                        .json(&next_req)
                                        .unwrap()
                                        .send()
                                        .await
                                    {
                                        Ok(response) if response.ok() => response
                                            .json::<Vec<serde_json::Value>>()
                                            .await
                                            .is_ok_and(|cards| !cards.is_empty()),
                                        _ => false,
                                    };
                                    if !next_ready {
                                        toast(
                                            &app_state,
                                            "Review saved, but the next card is unavailable",
                                        );
                                    }
                                } else {
                                    cards.set(
                                        cards
                                            .iter()
                                            .filter(|card| card.id != id)
                                            .cloned()
                                            .collect(),
                                    );
                                }
                            }
                            load_cards.emit(true);
                        }
                        _ => toast(&app_state, "Review failed"),
                    }
                });
            },
        )
    };

    let on_learn_more_cb = {
        let app_state = app_state.clone();
        let load_cards = load_cards.clone();
        Callback::from(move |(topic, tipcard_type): (String, String)| {
            let app_state = app_state.clone();
            let load_cards = load_cards.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = CreateTipReq {
                    count: Some(5),
                    topics: topic,
                    tipcard_type: Some(tipcard_type),
                    manual_content: None,
                    manual_image_data: None,
                    exclude_card_ids: None,
                };
                let loaded = match Request::post("/app/tips").json(&req).unwrap().send().await {
                    Ok(response) if response.ok() => response
                        .json::<Vec<serde_json::Value>>()
                        .await
                        .is_ok_and(|cards| !cards.is_empty()),
                    _ => false,
                };
                if loaded {
                    toast(&app_state, "More cards loaded");
                    load_cards.emit(true);
                } else {
                    toast(&app_state, "Could not load more cards");
                }
            });
        })
    };

    let on_toggle_pin_cb = {
        let cards = cards.clone();
        let card_order = card_order.clone();
        let pinned_card_order = pinned_card_order.clone();
        Callback::from(move |(id, pinned): (i64, bool)| {
            let cards = cards.clone();
            let card_order = card_order.clone();
            let pinned_card_order = pinned_card_order.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PinReq {
                    id,
                    pinned: Some(pinned),
                    image_data: None,
                };
                if let Ok(res) = Request::patch("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    if res.ok() {
                        let mut next = (*cards).clone();
                        if let Some(card) = next.iter_mut().find(|card| card.id == id) {
                            card.pinned = pinned;
                        }
                        let unpinned_ids: Vec<i64> = next
                            .iter()
                            .filter(|card| !card.pinned)
                            .map(|card| card.id)
                            .collect();
                        let pinned_ids: Vec<i64> = next
                            .iter()
                            .filter(|card| card.pinned)
                            .map(|card| card.id)
                            .collect();
                        cards.set(next);

                        if pinned {
                            let order = normalize_card_order((*card_order).clone(), &unpinned_ids);
                            let _ = LocalStorage::set("denpie-card-order", &order);
                            card_order.set(order);
                        } else {
                            let order =
                                normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
                            let _ = LocalStorage::set("denpie-pinned-card-order", &order);
                            pinned_card_order.set(order);
                        }
                    }
                }
            });
        })
    };

    let on_update_images_cb = {
        let request_detail = request_detail.clone();
        let app_state = app_state.clone();
        let detail_loaded = detail_loaded.clone();
        Callback::from(move |(id, image_data): (i64, Vec<String>)| {
            let request_detail = request_detail.clone();
            let app_state = app_state.clone();
            let detail_loaded = detail_loaded.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req = PinReq {
                    id,
                    pinned: None,
                    image_data: Some(image_data),
                };
                match Request::patch("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                {
                    Ok(res) if res.ok() => {
                        toast(&app_state, "Images updated");
                        let mut loaded = (*detail_loaded).clone();
                        loaded.insert(id, false);
                        detail_loaded.set(loaded);
                        request_detail.emit(id);
                    }
                    Ok(res) => toast(
                        &app_state,
                        res.text()
                            .await
                            .unwrap_or_else(|_| "Failed to update images".to_string()),
                    ),
                    Err(err) => toast(&app_state, err.to_string()),
                }
            });
        })
    };

    let on_delete_cb = {
        let app_state = app_state.clone();
        let cards = cards.clone();
        let reviewed_placeholders = reviewed_placeholders.clone();
        let fullscreen_card_key = fullscreen_card_key.clone();
        Callback::from(move |id: i64| {
            let app_state = app_state.clone();
            let cards = cards.clone();
            let reviewed_placeholders = reviewed_placeholders.clone();
            let fullscreen_card_key = fullscreen_card_key.clone();
            let deleted_card_key = cards.iter().find(|card| card.id == id).map(flow_card_key);
            wasm_bindgen_futures::spawn_local(async move {
                let req = serde_json::json!({ "id": id });
                if Request::delete("/admin/tipcards")
                    .json(&req)
                    .unwrap()
                    .send()
                    .await
                    .is_ok()
                {
                    toast(&app_state, "Card deleted");
                    if deleted_card_key == *fullscreen_card_key {
                        set_fullscreen_body_class(false);
                        fullscreen_card_key.set(None);
                    }
                    cards.set(cards.iter().filter(|card| card.id != id).cloned().collect());
                    let mut placeholders = (*reviewed_placeholders).clone();
                    placeholders.remove(&id);
                    reviewed_placeholders.set(placeholders);
                }
            });
        })
    };

    let on_reorder_cb = {
        let card_order = card_order.clone();
        let pinned_card_order = pinned_card_order.clone();
        let cards = cards.clone();
        let sort_by = sort_by.clone();
        Callback::from(move |(source_id, target_id): (i64, i64)| {
            let source_pinned = cards
                .iter()
                .find(|card| card.id == source_id)
                .map(|card| card.pinned);
            let target_pinned = cards
                .iter()
                .find(|card| card.id == target_id)
                .map(|card| card.pinned);
            let (Some(source_pinned), Some(target_pinned)) = (source_pinned, target_pinned) else {
                return;
            };
            if source_pinned != target_pinned {
                return;
            }

            if source_pinned {
                let pinned_ids: Vec<i64> = cards
                    .iter()
                    .filter(|card| card.pinned)
                    .map(|card| card.id)
                    .collect();
                let mut order = normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
                if let (Some(from_idx), Some(to_idx)) = (
                    order.iter().position(|&id| id == source_id),
                    order.iter().position(|&id| id == target_id),
                ) {
                    let item = order.remove(from_idx);
                    order.insert(to_idx, item);
                    let _ = LocalStorage::set("denpie-pinned-card-order", &order);
                    pinned_card_order.set(order);
                }
                return;
            }

            let unpinned_ids: Vec<i64> = cards
                .iter()
                .filter(|card| !card.pinned)
                .map(|card| card.id)
                .collect();
            let mut order = normalize_card_order((*card_order).clone(), &unpinned_ids);

            if let (Some(from_idx), Some(to_idx)) = (
                order.iter().position(|&id| id == source_id),
                order.iter().position(|&id| id == target_id),
            ) {
                let item = order.remove(from_idx);
                order.insert(to_idx, item);
                let _ = LocalStorage::set("denpie-card-order", &order);
                card_order.set(order);
                let _ = LocalStorage::set("denpie-flow-sort", "drag");
                sort_by.set("drag".to_string());
            }
        })
    };

    let on_toggle_fullscreen = {
        let fullscreen_card_key = fullscreen_card_key.clone();
        let request_detail = request_detail.clone();
        let cards = cards.clone();
        Callback::from(move |id: i64| {
            let Some(card_key) = cards.iter().find(|card| card.id == id).map(flow_card_key) else {
                return;
            };
            if *fullscreen_card_key == Some(card_key.clone()) {
                set_fullscreen_body_class(false);
                fullscreen_card_key.set(None);
            } else {
                set_fullscreen_body_class(true);
                request_detail.emit(id);
                fullscreen_card_key.set(Some(card_key));
            }
        })
    };

    let on_measure = {
        let card_heights = card_heights.clone();
        Callback::from(move |(id, height): (i64, f64)| {
            if height <= 0.0 {
                return;
            }
            let current = card_heights.get(&id).copied().unwrap_or(0.0);
            if (current - height).abs() > 2.0 {
                let mut next = (*card_heights).clone();
                next.insert(id, height);
                card_heights.set(next);
            }
        })
    };

    let mut pinned_cards: Vec<TipcardInfo> =
        cards.iter().filter(|card| card.pinned).cloned().collect();
    let mut unpinned_cards: Vec<TipcardInfo> =
        cards.iter().filter(|card| !card.pinned).cloned().collect();
    let pinned_ids: Vec<i64> = pinned_cards.iter().map(|card| card.id).collect();
    let unpinned_ids: Vec<i64> = unpinned_cards.iter().map(|card| card.id).collect();

    if !(*pinned_card_order).is_empty() {
        let normalized_pinned_order =
            normalize_card_order((*pinned_card_order).clone(), &pinned_ids);
        pinned_cards.sort_by_key(|card| {
            normalized_pinned_order
                .iter()
                .position(|&id| id == card.id)
                .unwrap_or(usize::MAX)
        });
    } else {
        sort_flow_cards(&mut pinned_cards, sort_by.as_str(), &[]);
    }

    sort_flow_cards(
        &mut unpinned_cards,
        sort_by.as_str(),
        &normalize_card_order((*card_order).clone(), &unpinned_ids),
    );

    let (transmission_picks, remaining_cards) = split_topic_picks(&unpinned_cards);
    let mut current_ids: Vec<i64> = pinned_cards
        .iter()
        .chain(transmission_picks.iter())
        .chain(remaining_cards.iter())
        .map(|card| card.id)
        .collect();

    // Fullscreen can target a card that topic-pick filtering would otherwise hide
    // (e.g. stacked active repeatables). Keep that card mounted so the
    // `is-fullscreen` node remains while `body.has-fullscreen-card` is set.
    let current_keys = pinned_cards
        .iter()
        .chain(transmission_picks.iter())
        .chain(remaining_cards.iter())
        .map(flow_card_key)
        .collect::<HashSet<_>>();
    let fullscreen_orphan = (*fullscreen_card_key).as_ref().and_then(|key| {
        if current_keys.contains(key) {
            None
        } else {
            cards
                .iter()
                .find(|card| flow_card_key(card) == *key)
                .cloned()
        }
    });
    if let Some(card) = fullscreen_orphan.as_ref() {
        current_ids.push(card.id);
    }

    {
        let fullscreen_card_key = fullscreen_card_key.clone();
        let card_keys = cards.iter().map(flow_card_key).collect::<HashSet<_>>();
        let route = use_route::<View>();
        let flow_active = route.as_ref() == Some(&View::Flow);
        use_effect_with(
            ((*fullscreen_card_key).clone(), card_keys, flow_active),
            move |(fullscreen, keys, active)| {
                // Keep-alive routes share `body.has-fullscreen-card`. Only the active
                // Flow view may own it, and only while its target card still exists.
                let should_lock = *active
                    && match fullscreen {
                        Some(key) if keys.contains(key) => true,
                        Some(_) => {
                            fullscreen_card_key.set(None);
                            false
                        }
                        None => false,
                    };
                set_fullscreen_body_class(should_lock);
                move || {
                    set_fullscreen_body_class(false);
                }
            },
        );
    }

    let list_mode = *layout == "list";
    let visible_card_count = current_ids.len();
    let disable_flow_glass =
        should_disable_flow_glass(list_mode, visible_card_count, &card_heights, &current_ids);

    {
        let load_cards = load_cards.clone();
        let has_more = *has_more;
        let loading = *loading;
        let loaded_count = cards.len();
        use_effect_with((has_more, loading, loaded_count), move |_| {
            if has_more && !loading && loaded_count == 0 {
                load_cards.emit(false);
            }
            || ()
        });
    }

    let on_manual_images = {
        let manual_images = manual_images.clone();
        let app_state = app_state.clone();
        Callback::from(move |e: Event| {
            let Some(input) = e.target_dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(files) = input.files() else {
                return;
            };
            if files.length() == 0 {
                return;
            }
            let selected = collect_files(&files);
            input.set_value("");
            let manual_images = manual_images.clone();
            let app_state = app_state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match compress_files_to_data_urls(selected).await {
                    Ok(mut compressed) => {
                        let mut next = (*manual_images).clone();
                        next.append(&mut compressed);
                        manual_images.set(next);
                    }
                    Err(message) => toast(&app_state, message),
                }
            });
        })
    };
    let on_upload_error = {
        let app_state = app_state.clone();
        Callback::from(move |message: String| toast(&app_state, message))
    };
    let on_flow_dragover = Callback::from(|e: DragEvent| {
        e.prevent_default();
        auto_scroll_for_drag(&e);
    });
    let render_flow_card = |card: &TipcardInfo| {
        let card = card.clone();
        let id = card.id;
        let card_key = flow_card_key(&card);
        html! {
            <FlowCard
                key={card_key.clone()}
                card={card}
                on_review={on_review_cb.clone()}
                on_learn_more={on_learn_more_cb.clone()}
                on_toggle_pin={on_toggle_pin_cb.clone()}
                on_delete={on_delete_cb.clone()}
                on_reorder={on_reorder_cb.clone()}
                on_update_images={on_update_images_cb.clone()}
                on_upload_error={on_upload_error.clone()}
                on_toggle_fullscreen={on_toggle_fullscreen.clone()}
                on_request_detail={request_detail.clone()}
                on_measure={on_measure.clone()}
                list_mode={list_mode}
                fullscreen={*fullscreen_card_key == Some(card_key.clone())}
                detail_loaded={detail_loaded.get(&id).copied().unwrap_or(false)}
            />
        }
    };
    let grid_classes = if list_mode {
        "grid grid-cols-1 gap-3 items-start w-full max-w-4xl mx-auto"
    } else {
        "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-3 items-start"
    };

    html! {
        <section
            id="view-flow"
            class={classes!(disable_flow_glass.then_some("flow-many-cards"))}
        >
            <div class="flex flex-col xl:flex-row xl:items-end justify-between gap-3 mb-4">
                <div>
                    <h1 class="text-xl font-semibold tracking-tight">{"Transmission"}</h1>
                    <p class="text-muted mt-2">{"All cards in one review surface."}</p>
                </div>
                <form id="tips-form" onsubmit={on_submit} class="surface border rounded-md p-4 grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-5 gap-3 w-full xl:w-auto">
                    <input
                        id="tips-topics"
                        class="rounded-md border px-3 py-2 xl:col-span-2"
                        placeholder="Rust, Python, System Design"
                        value={(*topics_input).clone()}
                        oninput={Callback::from({let t = topics_input.clone(); move |e: InputEvent| if let Some(target) = e.target_dyn_into::<HtmlInputElement>() { t.set(target.value()); }})}
                        onkeydown={Callback::from({
                            let tip_type = tip_type.clone();
                            move |e: KeyboardEvent| {
                                if *tip_type == "manual_tip" && e.key() == "Tab" && !e.shift_key() {
                                    if let Some(window) = web_sys::window() {
                                        if let Some(document) = window.document() {
                                            if let Some(el) = document.get_element_by_id("manual-card-content") {
                                                if let Ok(textarea) = el.dyn_into::<HtmlTextAreaElement>() {
                                                    let _ = textarea.focus();
                                                    e.prevent_default();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        })}
                        required=true
                    />
                    <div class="tip-type-switch muted-surface border border-token rounded-md p-1 grid grid-cols-3 sm:col-span-2" role="group">
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("casual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "casual_tip").then_some("active"))}>{"Casual"}</button>
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("repeatable_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", (*tip_type == "repeatable_tip").then_some("active"))}>{"Repeat"}</button>
                        <button type="button" onclick={let t = tip_type.clone(); Callback::from(move |_| t.set("manual_tip".to_string()))} class={classes!("rounded-md", "px-3", "py-2", "text-sm", "font-medium", "inline-flex", "items-center", "justify-center", "gap-1.5", (*tip_type == "manual_tip").then_some("active"))}>
                            {"Manual"}
                        </button>
                    </div>
                    <button
                        id="tips-submit-btn"
                        type="submit"
                        class={classes!("rounded-md", "bg-primary-solid", "px-4", "py-2", "font-medium", "flex", "items-center", "justify-center", "gap-2", (*pending_count > 0).then_some("opacity-60 cursor-not-allowed"))}
                        disabled={*pending_count > 0}
                    >
                        <iconify-icon icon={if *pending_count > 0 { "radix-icons:update" } else if *tip_type == "manual_tip" { "material-symbols:accessible-forward" } else { "radix-icons:magic-wand" }} class={classes!("radix-icon", (*pending_count > 0).then_some("animate-spin"))} aria-hidden="true"></iconify-icon>
                        <span>{ if *pending_count > 0 { "Adding..." } else { "Add" } }</span>
                    </button>
                    if *tip_type == "manual_tip" {
                        <textarea
                            id="manual-card-content"
                            class="rounded-md border px-3 py-2 sm:col-span-2 xl:col-span-5 h-20 resize-y"
                            placeholder="Manual card content"
                            value={(*manual_content).clone()}
                            oninput={Callback::from({ let manual_content = manual_content.clone(); move |e: InputEvent| if let Some(target) = e.target_dyn_into::<HtmlTextAreaElement>() { manual_content.set(target.value()); }})}
                            onkeydown={Callback::from({
                                move |e: KeyboardEvent| {
                                    if e.key() == "Enter" && e.shift_key() {
                                        e.prevent_default();
                                        if let Some(window) = web_sys::window() {
                                            if let Some(document) = window.document() {
                                                if let Some(btn) = document.get_element_by_id("tips-submit-btn") {
                                                    if let Ok(btn_el) = btn.dyn_into::<web_sys::HtmlElement>() {
                                                        btn_el.click();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            })}
                        ></textarea>
                        <div class="sm:col-span-2 xl:col-span-5 flex flex-wrap items-center gap-3">
                            <label class="inline-flex items-center gap-2 rounded-md border border-token px-3 py-2 text-sm font-medium cursor-pointer">
                                <iconify-icon icon="radix-icons:image" class="radix-icon"></iconify-icon>
                                <span>{"Add images"}</span>
                                <input id="manual-card-images" type="file" multiple=true accept="image/*" class="hidden" onchange={on_manual_images} />
                            </label>
                            <span class="text-sm text-muted">{format!("{} images", manual_images.len())}</span>
                            if !manual_images.is_empty() {
                                <button type="button" class="rounded-md border border-token px-3 py-2 text-sm" onclick={Callback::from({ let manual_images = manual_images.clone(); move |_| manual_images.set(Vec::new()) })}>{"Clear"}</button>
                            }
                        </div>
                    }
                </form>
            </div>

            if !pinned_cards.is_empty() {
                <section id="flow-pins" class="mb-8" aria-labelledby="flow-pins-heading">
                    <div class="flex items-center justify-between gap-3 mb-4">
                        <h2 id="flow-pins-heading" class="text-lg font-semibold tracking-tight">
                            {i18n.t("flow.pins")}
                        </h2>
                        <span id="flow-pinned-count" class="text-sm text-muted">
                            {pinned_cards.len()}
                        </span>
                    </div>
                    <div
                        id="flow-pinned-grid"
                        class={grid_classes}
                        ondragover={on_flow_dragover.clone()}
                    >
                        {for pinned_cards.iter().map(&render_flow_card)}
                    </div>
                </section>
            }

            <div class="flex justify-between items-center gap-3 mb-4">
                <div>
                    <h2 id="flow-picks-heading" class="text-lg font-semibold tracking-tight">
                        {i18n.t("flow.picks")}
                    </h2>
                    <div class="text-sm text-muted mt-1">
                        <span id="flow-count">{transmission_picks.len()}</span>
                        {format!("/{TRANSMISSION_MAX_PICKS} {}", i18n.t("flow.picks_count_suffix"))}
                    </div>
                </div>
                <div class="flex flex-wrap items-center justify-end gap-2">
                    <div class="flex muted-surface rounded-md p-1 border border-token" role="group" aria-label="Sort cards">
                        <button
                            type="button"
                            class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "topic").then_some("bg-primary-soft text-primary"))}
                            aria-pressed={(*sort_by == "topic").to_string()}
                            onclick={Callback::from({
                                let sort_by = sort_by.clone();
                                move |_| {
                                    let _ = LocalStorage::set("denpie-flow-sort", "topic");
                                    sort_by.set("topic".to_string());
                                }
                            })}
                        >
                            {"Topic"}
                        </button>
                        <button
                            type="button"
                            class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "date").then_some("bg-primary-soft text-primary"))}
                            aria-pressed={(*sort_by == "date").to_string()}
                            onclick={Callback::from({
                                let sort_by = sort_by.clone();
                                move |_| {
                                    let _ = LocalStorage::set("denpie-flow-sort", "date");
                                    sort_by.set("date".to_string());
                                }
                            })}
                        >
                            {"Date"}
                        </button>
                        <button
                            type="button"
                            class={classes!("rounded", "px-2", "py-1", "text-sm", "font-medium", (*sort_by == "drag").then_some("bg-primary-soft text-primary"))}
                            aria-pressed={(*sort_by == "drag").to_string()}
                            onclick={Callback::from({
                                let sort_by = sort_by.clone();
                                move |_| {
                                    let _ = LocalStorage::set("denpie-flow-sort", "drag");
                                    sort_by.set("drag".to_string());
                                }
                            })}
                        >
                            {"Drag"}
                        </button>
                    </div>
                    <div class="flex muted-surface rounded-md p-1 border border-token">
                        <button id="flow-grid-btn" type="button" class={classes!("rounded", "px-2", "py-1", (!list_mode).then_some("bg-primary-soft text-primary"))} onclick={Callback::from({ let layout = layout.clone(); move |_| { let _ = LocalStorage::set("denpie-flow-layout", "grid"); layout.set("grid".to_string()); } })}>
                            <iconify-icon icon="radix-icons:grid" class="radix-icon"></iconify-icon>
                        </button>
                        <button id="flow-list-btn" type="button" class={classes!("rounded", "px-2", "py-1", list_mode.then_some("bg-primary-soft text-primary"))} onclick={Callback::from({ let layout = layout.clone(); move |_| { let _ = LocalStorage::set("denpie-flow-layout", "list"); layout.set("list".to_string()); } })}>
                            <iconify-icon icon="radix-icons:list-bullet" class="radix-icon"></iconify-icon>
                        </button>
                    </div>
                </div>
            </div>

            <div
                id="flow-grid"
                class={grid_classes}
                aria-labelledby="flow-picks-heading"
                ondragover={on_flow_dragover.clone()}
            >
                {
                    for (0..*pending_count).map(|i| html! {
                        <FlowCardSkeleton key={format!("skeleton-{i}")} list_mode={list_mode} />
                    })
                }
                {
                    for transmission_picks.iter().map(render_flow_card)
                }
            </div>

            if !remaining_cards.is_empty() {
                <section id="flow-other-cards" class="mt-8" aria-labelledby="flow-other-cards-heading">
                    <div class="flex items-center justify-between gap-3 mb-4">
                        <h2 id="flow-other-cards-heading" class="text-lg font-semibold tracking-tight">
                            {i18n.t("flow.other_cards")}
                        </h2>
                        <span id="flow-other-count" class="text-sm text-muted">
                            {remaining_cards.len()}
                        </span>
                    </div>
                    <div
                        id="flow-other-grid"
                        class={grid_classes}
                        ondragover={on_flow_dragover.clone()}
                    >
                        {for remaining_cards.iter().map(render_flow_card)}
                    </div>
                </section>
            }

            if *loading {
                <div class="flex justify-center py-8 text-sm text-muted">{"Loading cards..."}</div>
            } else if *has_more {
                <div class="flex justify-center py-8">
                    <button type="button" class="rounded-md border border-token px-6 py-2 font-medium" onclick={Callback::from({ let load_cards = load_cards.clone(); move |_| load_cards.emit(false) })}>{"Load More Cards"}</button>
                </div>
            }

            if visible_card_count == 0 && !*loading {
                <div id="empty-flow" class="surface border rounded-md p-10 text-center text-muted">
                    {"No cards yet."}
                </div>
            }

            if let Some(card) = fullscreen_orphan.as_ref() {
                { render_flow_card(card) }
            }
        </section>
    }
}

const FLOW_GLASS_GRID_THRESHOLD: usize = 8;
const FLOW_GLASS_LIST_VIEWPORT_MULTIPLIER: usize = 3;

fn flow_grid_columns(viewport_width: f64) -> usize {
    if viewport_width >= 1536.0 {
        4
    } else if viewport_width >= 1280.0 {
        3
    } else if viewport_width >= 768.0 {
        2
    } else {
        1
    }
}

fn estimate_visible_card_slots(
    list_mode: bool,
    card_heights: &HashMap<i64, f64>,
    card_ids: &[i64],
) -> usize {
    let Some(window) = web_sys::window() else {
        return if list_mode { 3 } else { 6 };
    };
    let viewport_h = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(800.0);
    let viewport_w = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(1024.0);

    let cols = if list_mode {
        1
    } else {
        flow_grid_columns(viewport_w)
    };
    let gap_px = 12.0;
    let measured: Vec<f64> = card_ids
        .iter()
        .filter_map(|id| card_heights.get(id).copied())
        .filter(|height| *height > 0.0)
        .collect();
    let avg_card_h = if measured.is_empty() {
        if list_mode { 360.0 } else { 280.0 }
    } else {
        measured.iter().sum::<f64>() / measured.len() as f64
    };
    let rows = (viewport_h / (avg_card_h + gap_px)).ceil().max(1.0) as usize;
    cols * rows
}

fn should_disable_flow_glass(
    list_mode: bool,
    card_count: usize,
    card_heights: &HashMap<i64, f64>,
    card_ids: &[i64],
) -> bool {
    if card_count == 0 {
        return false;
    }
    if list_mode {
        let visible_slots = estimate_visible_card_slots(true, card_heights, card_ids);
        card_count > visible_slots.saturating_mul(FLOW_GLASS_LIST_VIEWPORT_MULTIPLIER)
    } else {
        card_count > FLOW_GLASS_GRID_THRESHOLD
    }
}

fn normalize_flow_sort(value: &str) -> String {
    match value {
        "manual" | "drag" => "drag".to_string(),
        "date" | "topic" => value.to_string(),
        _ => "topic".to_string(),
    }
}

fn sort_flow_cards(cards: &mut [TipcardInfo], sort_by: &str, drag_order: &[i64]) {
    match sort_by {
        "date" => cards.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        }),
        "drag" if !drag_order.is_empty() => cards.sort_by_key(|card| {
            drag_order
                .iter()
                .position(|&id| id == card.id)
                .unwrap_or(usize::MAX)
        }),
        _ => cards.sort_by(|a, b| {
            a.topic_name
                .to_lowercase()
                .cmp(&b.topic_name.to_lowercase())
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        }),
    }
}

fn flow_card_key(card: &TipcardInfo) -> String {
    if card.tipcard_type == "repeatable_tip" {
        format!("repeatable:{}", card.topic_name)
    } else {
        format!("card:{}", card.id)
    }
}

fn select_topic_picks(cards: &[TipcardInfo]) -> Vec<TipcardInfo> {
    let active_repeatable_topics = cards
        .iter()
        .filter(|card| card.tipcard_type == "repeatable_tip" && card.status == "active")
        .map(|card| card.topic_name.as_str())
        .collect::<HashSet<_>>();
    let topic_count = cards
        .iter()
        .filter(|card| {
            card.status == "active"
                || card.tipcard_type == "repeatable_tip"
                    && card.status == "reviewed"
                    && !active_repeatable_topics.contains(card.topic_name.as_str())
        })
        .map(|card| card.topic_name.as_str())
        .collect::<HashSet<_>>()
        .len();
    if topic_count == 0 {
        return Vec::new();
    }

    let per_topic_limit =
        TRANSMISSION_MAX_PICKS_PER_TOPIC.min((TRANSMISSION_MAX_PICKS / topic_count).max(1));
    let mut topic_counts = HashMap::<&str, usize>::new();
    let mut repeatable_topics = HashSet::<&str>::new();
    let mut picks = Vec::with_capacity(TRANSMISSION_MAX_PICKS.min(cards.len()));

    for card in cards {
        let repeatable_placeholder = card.tipcard_type == "repeatable_tip"
            && card.status == "reviewed"
            && !active_repeatable_topics.contains(card.topic_name.as_str());
        if card.status != "active" && !repeatable_placeholder {
            continue;
        }
        if card.tipcard_type == "repeatable_tip"
            && !repeatable_topics.insert(card.topic_name.as_str())
        {
            continue;
        }
        let count = topic_counts.entry(card.topic_name.as_str()).or_default();
        let card_limit = if card.tipcard_type == "repeatable_tip" {
            1
        } else {
            per_topic_limit
        };
        if *count >= card_limit {
            continue;
        }
        picks.push(card.clone());
        *count += 1;
        if picks.len() == TRANSMISSION_MAX_PICKS {
            break;
        }
    }

    picks
}

fn split_topic_picks(cards: &[TipcardInfo]) -> (Vec<TipcardInfo>, Vec<TipcardInfo>) {
    let picks = select_topic_picks(cards);
    let pick_ids = picks.iter().map(|card| card.id).collect::<HashSet<_>>();
    let remaining = cards
        .iter()
        .filter(|card| !(pick_ids.contains(&card.id) || card.tipcard_type == "repeatable_tip"))
        .cloned()
        .collect();
    (picks, remaining)
}

fn review_placeholder_message(action: &str, _previous_review_at: &str) -> String {
    match action {
        "again" | "repeat" => {
            "Saved for another review. It will return on its SM-2 schedule.".to_string()
        }
        "learned" | "memorize" => {
            "Marked as learned. It will return when SM-2 schedules it.".to_string()
        }
        "skip_known" => "Skipped as already known. The next card will build beyond it.".to_string(),
        "skip_too_difficult" => {
            "Skipped as too difficult. The next card will be an easier step.".to_string()
        }
        "skip_not_interested" | "dismiss" => {
            "Skipped as not interesting. Future cards will change direction.".to_string()
        }
        _ => "Review saved. This card will return on its schedule.".to_string(),
    }
}

fn normalize_card_order(mut order: Vec<i64>, current_ids: &[i64]) -> Vec<i64> {
    order.retain(|id| current_ids.contains(id));
    for id in current_ids {
        if !order.contains(id) {
            order.push(*id);
        }
    }
    order
}

fn auto_scroll_for_drag(event: &DragEvent) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(inner_height) = window.inner_height() else {
        return;
    };
    let Some(viewport_height) = inner_height.as_f64() else {
        return;
    };

    let pointer_y = event.client_y() as f64;
    let delta = if pointer_y < DRAG_SCROLL_EDGE_PX {
        -scroll_step(DRAG_SCROLL_EDGE_PX - pointer_y)
    } else if viewport_height - pointer_y < DRAG_SCROLL_EDGE_PX {
        scroll_step(DRAG_SCROLL_EDGE_PX - (viewport_height - pointer_y))
    } else {
        return;
    };

    window.scroll_by_with_x_and_y(0.0, delta);
}

fn set_fullscreen_body_class(fullscreen: bool) {
    let Some(body) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.body())
    else {
        return;
    };
    let _ = body
        .class_list()
        .toggle_with_force("has-fullscreen-card", fullscreen);
}

fn scroll_step(edge_overlap: f64) -> f64 {
    let intensity = (edge_overlap / DRAG_SCROLL_EDGE_PX).clamp(0.0, 1.0);
    (intensity * DRAG_SCROLL_MAX_STEP_PX).max(4.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: i64, topic_name: &str) -> TipcardInfo {
        TipcardInfo {
            id,
            topic_name: topic_name.to_string(),
            topic_icon: String::new(),
            topic_color: String::new(),
            title: format!("Card {id}"),
            full_content: String::new(),
            compressed_content: String::new(),
            image_data: Vec::new(),
            created_at: String::new(),
            tipcard_type: "casual_tip".to_string(),
            status: "active".to_string(),
            next_review_at: String::new(),
            repeat_count: 0,
            pinned: false,
            pending_count: 0,
            review_message: None,
        }
    }

    #[test]
    fn transmission_keeps_three_picks_for_non_repeatable_topics() {
        let cards = ["A", "B", "C"]
            .into_iter()
            .flat_map(|topic| (0..4).map(move |index| card(index, topic)))
            .collect::<Vec<_>>();

        let picks = select_topic_picks(&cards);

        assert_eq!(picks.len(), 9);
        for topic in ["A", "B", "C"] {
            assert_eq!(
                picks.iter().filter(|card| card.topic_name == topic).count(),
                3
            );
        }
    }

    #[test]
    fn repeatable_cards_stack_behind_one_topic_pick() {
        let cards = (0..3)
            .map(|index| {
                let mut card = card(index, "A");
                card.tipcard_type = "repeatable_tip".to_string();
                card
            })
            .collect::<Vec<_>>();

        let (picks, remaining) = split_topic_picks(&cards);

        assert_eq!(picks.len(), 1);
        assert!(remaining.is_empty());
    }

    #[test]
    fn reviewed_repeatable_holds_topic_pick_until_next_card_is_active() {
        let mut reviewed = card(1, "A");
        reviewed.tipcard_type = "repeatable_tip".to_string();
        reviewed.status = "reviewed".to_string();
        reviewed.review_message = Some("Review saved".to_string());

        let (picks, remaining) = split_topic_picks(&[reviewed]);

        assert_eq!(
            picks.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![1]
        );
        assert!(remaining.is_empty());
    }

    #[test]
    fn active_repeatable_replaces_placeholder_in_the_same_topic_slot() {
        let mut reviewed = card(1, "A");
        reviewed.tipcard_type = "repeatable_tip".to_string();
        reviewed.status = "reviewed".to_string();
        let mut active = card(2, "A");
        active.tipcard_type = "repeatable_tip".to_string();

        let (picks, remaining) = split_topic_picks(&[reviewed.clone(), active]);

        assert_eq!(
            picks.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![2]
        );
        assert!(remaining.is_empty());
        assert_eq!(flow_card_key(&reviewed), flow_card_key(&picks[0]));
    }

    #[test]
    fn transmission_never_shows_more_than_nine_topics() {
        let cards = (0..12)
            .map(|index| card(index, &format!("Topic {index}")))
            .collect::<Vec<_>>();

        let picks = select_topic_picks(&cards);

        assert_eq!(picks.len(), 9);
        assert!(picks.iter().all(|pick| {
            picks
                .iter()
                .filter(|other| other.topic_name == pick.topic_name)
                .count()
                == 1
        }));
    }

    #[test]
    fn reviewed_repeatable_is_hidden_when_its_topic_has_an_active_card() {
        let active = card(1, "A");
        let mut reviewed = card(2, "A");
        reviewed.tipcard_type = "repeatable_tip".to_string();
        reviewed.status = "reviewed".to_string();
        reviewed.review_message = Some("Review saved".to_string());
        let cards = vec![active, reviewed];

        let (picks, remaining) = split_topic_picks(&cards);

        assert_eq!(
            picks.iter().map(|card| card.id).collect::<Vec<_>>(),
            vec![1]
        );
        assert!(remaining.is_empty());
    }
}
